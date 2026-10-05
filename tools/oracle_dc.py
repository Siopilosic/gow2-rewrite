"""Oracle summaries for the Rust DC move-graph reader: one row per move with counts and a checksum of its branches, hit
windows and actions, plus one row for the entry branches of CRT_Hero.

  python tools/oracle_dc.py gow2-rs/crates/gow2-formats/tests/oracle/dc.tsv

Independent of the Rust code: it uses the field offsets of tools/dcmoves.py. The Rust test (tests/dc_oracle.rs) must reproduce
every row. Only derived counts and checksums are stored, no game data.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import DC, gow_hash  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WADS = ["R_HERO00"]
M64 = 0xFFFFFFFFFFFFFFFF


def fnv(h, x):
    return ((h ^ (x & M64)) * 1099511628211) & M64


def q(x):
    return int(round(x * 10000))


def run(wad):
    dc = DC(os.path.join(ROOT, "extracted", "pak", wad + ".WAD"))
    B = dc.blob
    objs = sorted(dc.objects)
    info = {o: (n, t) for o, n, t in objs}
    mov_offs = [o for o, n, t in objs if t == 0x62]
    hnames = {h: n for h, n in dc.hashes}

    def u32(o):
        return struct.unpack_from("<I", B, o)[0]

    def rel(o):
        v = struct.unpack_from("<i", B, o)[0]
        return None if v == 0 else o + v

    def plist(o):
        w = struct.unpack_from("<i", B, o)[0]
        n = w & 0xFFF
        if not n:
            return []
        arr = o + (w >> 12)
        return [rel(arr + 4 * k) for k in range(n)]

    def half(o):
        return struct.unpack_from("<e", B, o)[0]

    def name_hash(o):
        return gow_hash(info[o][0]) if o in info else 0

    def branch_sum(h, b):
        t = rel(b)
        for x in (name_hash(t) if t in info and info[t][1] == 0x62 else 0, u32(b + 8), u32(b + 12), q(half(b + 0x10)), q(half(b + 0x12)),
                  q(half(b + 0x14)), *struct.unpack_from("<4h", B, b + 0x16), *struct.unpack_from("<2b", B, b + 0x1e),
                  B[b + 0x20], B[b + 0x21], B[b + 0x22], B[b + 0x23], struct.unpack_from("<b", B, b + 0x24)[0],
                  int(rel(b + 4) is not None)):
            h = fnv(h, x)
        return h

    rows = []
    for o in mov_offs:
        h = 1469598103934665603
        brs = [x for x in plist(o + 0x10) if x is not None]
        hits = [x for x in plist(o + 0x14) if x is not None]
        acts = [x for x in plist(o + 0x18) if x is not None]
        for b in brs:
            h = branch_sum(h, b)
        for c in hits:
            for x in (q(half(c)), q(half(c + 2)), q(half(c + 0x16)), B[c + 0x18], B[c + 0x19], struct.unpack_from("<b", B, c + 0x1a)[0],
                      *[q(half(c + k)) for k in (4, 6, 8, 0xa, 0xc, 0xe, 0x10, 0x12, 0x14)]):
                h = fnv(h, x)
        for a in acts:
            for x in (B[a], B[a + 1], B[a + 2], struct.unpack_from("<b", B, a + 3)[0], q(half(a + 4)), q(half(a + 6))):
                h = fnv(h, x)
        rows.append((f"{o:x}", info[o][0], hnames.get(u32(o + 8), ""), len(brs), len(hits), len(acts), q(half(o)), q(half(o + 2)), f"{u32(o + 4):x}", h))
    crt = next(o for o, n, t in objs if n == "CRT_Hero")
    ent = [x for x in plist(crt + 0x18) if x is not None and x in info and info[x][1] in (0x5f, 0x60)]
    h = 1469598103934665603
    for b in ent:
        h = branch_sum(h, b)
    rows.append(("entry", "CRT_Hero", "", len(ent), 0, 0, 0, 0, "0", h))
    return rows


rows = []
for w in WADS:
    for r in run(w):
        rows.append((w,) + r)
with open(sys.argv[1], "w", encoding="utf-8", newline="") as f:
    f.write("wad\toff\tname\tanim\tbranches\thits\tactions\trate\tblend\tflags\tcheck\n")
    for r in rows:
        f.write("\t".join(str(x) for x in r) + "\n")
print(len(rows), "rows")
