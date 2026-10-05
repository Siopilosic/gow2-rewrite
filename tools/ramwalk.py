"""Walk the engine-core runtime structures in EE RAM and compare them with the static model.

  python tools/ramwalk.py <source> report <label>   writes analysis/runtime/<label>/ (never overwrites)
  python tools/ramwalk.py <source> servers|master|pools|go
  python tools/ramwalk.py pine snapshot <label>      saves the full 32 MB EE RAM via PINE

<source> is `pine[:port]`, a PCSX2 savestate (.p2s) or a raw 32 MB RAM image (see eeram.py).

Every check prints OK / MISMATCH against the model item it tests (docs/confirmed.md ids).
Offsets used here are the ones recorded in analysis/structs.txt and confirmed.md; nothing is
inferred from the RAM contents themselves.
"""
import csv
import datetime
import hashlib
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eeram import Mem, open_ram  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER_TABLE = 0x00362C48
MASTER_ID = 5
GO_ID = 1
WAD_ID = 0x16


def load_classes():
    """vptr -> class name, from analysis/servers.tsv + vt_* symbols."""
    names = {}
    with open(os.path.join(ROOT, "analysis", "servers.tsv"), encoding="utf-8-sig") as f:
        for r in csv.DictReader(f, delimiter="\t"):
            names[int(r["final_vtable"], 16)] = (r["name"], int(r["id"], 16))
    for line in open(os.path.join(ROOT, "analysis", "symbols.tsv"), encoding="utf-8-sig"):
        c = line.rstrip("\n").split("\t")
        if len(c) > 2 and c[1] == "data" and c[2].startswith("vt_"):
            names.setdefault(int(c[0], 16), (c[2][3:], None))
    return names


class Walker:
    def __init__(self, mem, out):
        self.m = mem
        self.out = out
        self.classes = load_classes()
        self.ok = 0
        self.bad = 0

    def p(self, s=""):
        self.out.append(s)
        print(s)

    def check(self, cond, item, what):
        if cond:
            self.ok += 1
        else:
            self.bad += 1
        self.p(f"  [{'OK' if cond else 'MISMATCH'}] {item}: {what}")

    def cls(self, obj):
        vp = self.m.u32(obj + 0x20)
        return vp, self.classes.get(vp, (f"vt_{vp:08x}", None))[0]

    def rec_name(self, payload):
        if not self.m.valid_ptr(payload):
            return None
        return self.m.cstr(payload - 0x18)

    # ---- g_ServerTable ------------------------------------------------------------------------
    def servers(self):
        m = self.m
        self.p("== g_ServerTable @0x00362c48 (C-C1, C-C3)")
        table = {}
        for i in range(256):
            ptr = m.u32(SERVER_TABLE + 4 * i)
            if not ptr:
                continue
            table[i] = ptr
            tw = m.u32(ptr)
            vp, name = self.cls(ptr)
            reg = (tw >> 16) & 0xFFF
            self.p(f"  [{i:#04x}] {ptr:08x} type={tw:08x} flags={m.u16(ptr + 4):04x} "
                   f"order={m.u32(ptr + 0x10):08x} vptr={vp:08x} {name}")
            if i == 0:
                self.check(vp == 0x002F6968, "C-C1", "slot 0 is the root object (vptr 0x2f6968)")
            else:
                self.check(reg == i, "C-C3", f"registered at (type_word>>16)&0xfff = {reg:#x}")
                known = self.classes.get(vp)
                if known and known[1] is not None:
                    self.check(known[1] == i, "servers.tsv", f"class {known[0]} has id {known[1]:#x}")
        self.p(f"  {len(table)} non-null entries")
        return table

    # ---- Master child list --------------------------------------------------------------------
    def master(self, table):
        m = self.m
        self.p("== Master children (C-C6): list order = per-frame update order")
        master = table.get(MASTER_ID)
        if not master:
            self.p("  no server 5 registered yet")
            return
        sentinel = master + 0x24
        node = m.u32(sentinel)
        keys = []
        n = 0
        while node != sentinel and n < 300:
            child = node - 8
            vp, name = self.cls(child)
            tw = m.u32(child)
            flags = m.u16(child + 4)
            key = m.u32(child + 0x10)
            keys.append(key)
            self.p(f"  {n:2d} {child:08x} id={tw >> 16 & 0xfff:#04x} order={key:08x} flags={flags:04x} {name}"
                   + ("  [skip 0x10]" if flags & 0x10 else "") + ("  [remove]" if flags & 3 else ""))
            node = m.u32(node)
            n += 1
        self.check(n == m.u32(master + 0x30), "C-C6", f"child_count +0x30 = {m.u32(master + 0x30)} vs {n} walked")
        self.check(all(a >= b for a, b in zip(keys, keys[1:])), "C-C6", "order_key non-increasing along the list")

    # ---- pooled servers -----------------------------------------------------------------------
    def pools(self, table):
        m = self.m
        self.p("== Pooled servers (C-C7, C-C8)")
        for i, srv in sorted(table.items()):
            if i == 0:
                continue
            vp, name = self.cls(srv)
            nb = m.u32(srv + 0x40)
            top = m.s8(srv + 0xC8)
            if not (1 <= nb <= 16 and -1 <= top < 32):
                continue  # not a pooled server layout
            buckets = m.u32(srv + 0x24)
            cur = m.u32(srv + 0x44)
            dflt = m.u32(srv + 0xD4)
            stack = [m.u32(srv + 0x48 + 4 * k) for k in range(top + 1)]
            self.p(f"  [{i:#04x}] {name}: buckets={nb} cur={cur} default_bank_index={dflt} ctx_top={top} "
                   f"ctx=[{', '.join(f'{c:08x}' for c in stack)}]")
            for b in range(nb):
                bk = buckets + 12 * b
                items, cnt, cap = m.u32(bk), m.u32(bk + 4), m.u32(bk + 8)
                self.p(f"     bucket {b}: count={cnt} capacity={cap} items@{items:08x}")
                if b == cur and m.valid_ptr(items) and dflt < cnt:
                    bank = m.u32(items + 4 * dflt)
                    bvp, bname = self.cls(bank)
                    self.p(f"     default bank {bank:08x} vptr={bvp:08x} {bname}")
                    if i == GO_ID:
                        self.check(bvp == 0x002F6530, "C-D6", "GO default bank has vptr 0x2f6530")

    # ---- GameObject contexts and node trees -----------------------------------------------------
    def go(self, table, depth_limit=6, node_limit=400):
        m = self.m
        self.p("== GOServer contexts and node trees (H-GO*, FUN_00283510 / FUN_00140ff8)")
        srv = table.get(GO_ID)
        if not srv:
            self.p("  GOServer not registered")
            return
        buckets = m.u32(srv + 0x24)
        cur, dflt = m.u32(srv + 0x44), m.u32(srv + 0xD4)
        bank = m.u32(m.u32(buckets + 12 * cur) + 4 * dflt)
        wad_cur = None
        if WAD_ID in table:
            ws = table[WAD_ID]
            top = m.s8(ws + 0xC8)
            wad_cur = m.u32(ws + 0x48 + 4 * top) if top >= 0 else None
        self.p(f"  GO default bank {bank:08x}; WadServer current context {wad_cur and f'{wad_cur:08x}'}")
        node = m.u32(bank + 0x24)
        n = 0
        while m.valid_ptr(node) and node != bank + 0x24 and n < 2000:
            ctx = node - 8
            vp, name = self.cls(ctx)
            flags = m.u16(ctx + 4)
            desc = m.u32(ctx + 0x7C)  # 0x54 descriptor (GOBank_NewDescriptor): hdr->name at +0x34
            rname = m.cstr(desc + 0x34) if m.valid_ptr(desc) else None
            root = m.u32(ctx + 0x28)
            self.p(f"  ctx {ctx:08x} {rname or '?'} desc={desc:08x} desc+0x18={m.u32(desc + 0x18):08x} vptr={vp:08x} {name} flags={flags:04x} "
                   f"active(+0x70)={m.u32(ctx + 0x70):x} wad(+0x88)={m.u32(ctx + 0x88):08x} "
                   f"root(+0x28)={root:08x} node_top(+0x6c)={m.s8(ctx + 0x6C)}")
            if vp in (0x002F6610,):
                self.check(m.s8(ctx + 0x6C) >= 0 and m.u32(ctx + 0x2C) == root, "FUN_00283410",
                           "node stack[0] (+0x2c) is the root node")
            if m.valid_ptr(root):
                self.tree(root, 1, depth_limit, [0], node_limit)
            node = m.u32(node)
            n += 1
        self.p(f"  {n} contexts")

    def tree(self, node, depth, limit, count, node_limit):
        m = self.m
        lst = m.u32(node + 0xB4)
        if not m.valid_ptr(lst):
            return
        cell = m.u32(lst)
        guard = 0
        while cell != lst and m.valid_ptr(cell) and guard < 512 and count[0] < node_limit:
            child = m.u32(cell + 8)
            if not m.valid_ptr(child):
                break
            count[0] += 1
            tw = m.u32(child)
            flags = m.u16(child + 4)
            rec = m.u32(child + 0x1C)  # 0x70 go* node object: name[24] at +8
            name = m.cstr(rec + 8) if m.valid_ptr(rec) else None
            t20 = struct.unpack("<3f", m.ram.read(child + 0x50, 12))
            t70 = struct.unpack("<3f", m.ram.read(child + 0xA0, 12))
            self.p(f"  {'  ' * depth}- {child:08x} {name or '?'} type={tw:08x} flags={flags:04x} "
                   f"uid={m.u32(child + 0x68)} m20.t=({t20[0]:.2f},{t20[1]:.2f},{t20[2]:.2f}) "
                   f"m70.t=({t70[0]:.2f},{t70[1]:.2f},{t70[2]:.2f})")
            if depth < limit:
                self.tree(child, depth + 1, limit, count, node_limit)
            cell = m.u32(cell)
            guard += 1


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return
    src, cmd = sys.argv[1], sys.argv[2]
    ram = open_ram(src)
    if cmd == "snapshot":
        label = sys.argv[3]
        d = os.path.join(ROOT, "analysis", "runtime", label)
        os.makedirs(d, exist_ok=False)
        data = ram.read_block(0, 32 * 1024 * 1024)
        open(os.path.join(d, "ee.bin"), "wb").write(data)
        meta = dict(ram.info(), taken=datetime.datetime.now().isoformat(timespec="seconds"),
                    sha256=hashlib.sha256(data).hexdigest())
        json.dump(meta, open(os.path.join(d, "meta.json"), "w"), indent=1)
        print(f"saved {d}\\ee.bin {meta}")
        return
    out = []
    w = Walker(Mem(ram), out)
    w.p(f"source: {json.dumps(ram.info())}")
    table = w.servers()
    if cmd in ("master", "report"):
        w.master(table)
    if cmd in ("pools", "report"):
        w.pools(table)
    if cmd in ("go", "report"):
        w.go(table)
    w.p(f"checks: {w.ok} OK, {w.bad} MISMATCH")
    if cmd == "report":
        d = os.path.join(ROOT, "analysis", "runtime", sys.argv[3])
        os.makedirs(d, exist_ok=False)
        open(os.path.join(d, "report.txt"), "w", encoding="utf-8").write("\n".join(out) + "\n")
        print(f"-> {d}\\report.txt")


if __name__ == "__main__":
    main()
