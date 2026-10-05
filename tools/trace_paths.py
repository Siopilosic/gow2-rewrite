"""Static WAD-record -> object creation paths for every pooled server.

Model (each step verified by hand for GOServer and TextureServer, see docs/confirmed.md):
  Object record  -> tag handler 0x00185748 -> server = g_ServerTable[type & 0xffff]
  server.slot10 (base 0x0027c0b0): bank = pool[server.slot19(data)]; bank.slot10(rec, data)
  server.slot19: data[0] < 0 (instance record) -> default bank (server+0xd4)
                 else -> current context (server+0x48[(s8)server+0xc8])->field_1c
  default bank  = created by server.slot22
  context banks = created by default_bank.slot19 (alloc + ctor) when the context is activated

For each server this prints the default-bank class (vtables installed in server.slot22),
its slot10/slot19 functions, and the vtables installed by default_bank.slot19 (directly
or by a constructor it calls) together with that class's slot10 (resource creation).
Classes that cannot be resolved statically are reported as '?'.

  python trace_paths.py > analysis/wad_object_paths.tsv
"""
import csv
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, resolve_pairs, func_end  # noqa: E402
from server_profile import ALL, func_sizes, vtables_set  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def slot(img, vt, i):
    return img.word(vt + i * 8 + 4)


def body(img, sizes, a):
    return a, sizes.get(a) or (func_end(img, a) - a)


def vtables_and_calls(img, sizes, a, vts):
    start, n = body(img, sizes, a)
    found, calls = [], []
    for x, w, m, ops, res in resolve_pairs(img, start, start + n):
        if res in vts and res not in found:
            found.append(res)
        if m == "jal":
            calls.append(((x + 4) & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
    return found, calls


def installed(img, sizes, a, vts, depth=1):
    found, calls = vtables_and_calls(img, sizes, a, vts)
    if depth:
        for c in calls:
            if c in (0x0014AAA8, 0x0027B9A0):  # operator new, identity helper
                continue
            for v in installed(img, sizes, c, vts, depth - 1):
                if v not in found:
                    found.append(v)
    return [v for v in found if v != 0x002F6A48]


def main():
    img = Image()
    sizes = func_sizes()
    vts = vtables_set()
    rows = list(csv.DictReader(open(os.path.join(ROOT, "analysis", "servers.tsv"), encoding="utf-8-sig"),
                               delimiter="\t"))
    print("id\tserver\tserver_vt\tslot19\tslot22\tdefault_bank_vt\tdefault_slot10\tdefault_slot19\t"
          "context_bank_vts\tcontext_slot10")
    for r in rows:
        vt = int(r["final_vtable"], 16)
        s19, s22 = slot(img, vt, 19), slot(img, vt, 22)
        dvt = installed(img, sizes, s22, vts) if s22 else []
        dvt = dvt[-1] if dvt else None
        d10 = slot(img, dvt, 10) if dvt else None
        d19 = slot(img, dvt, 19) if dvt else None
        cvts = installed(img, sizes, d19, vts) if d19 else []
        c10 = ",".join(f"{slot(img, v, 10):08x}" for v in cvts if int(vts[v]["slots"]) > 10)
        f = lambda x: f"{x:08x}" if x else "?"
        print(f"{r['id']}\t{r['name']}\t{f(vt)}\t{f(s19)}\t{f(s22)}\t{f(dvt)}\t{f(d10)}\t{f(d19)}\t"
              f"{','.join(f'{v:08x}' for v in cvts) or '?'}\t{c10 or '?'}")


if __name__ == "__main__":
    main()
