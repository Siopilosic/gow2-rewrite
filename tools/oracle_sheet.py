"""Oracle summaries for the Rust collision-sheet reader: counts, surface names and a checksum per level WAD.

  python tools/oracle_sheet.py gow2-rs/crates/gow2-formats/tests/oracle/sheet.tsv

The Rust test (tests/sheet_oracle.rs) must reproduce every number. Only derived counts are stored, no game data.
"""
import glob
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from sheet_decode import find  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def fbits(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def checksum(polys):
    h = 1469598103934665603
    for s, corners, n in polys:
        seq = [s, len(corners)]
        for c in corners:
            seq.extend(fbits(v) for v in c)
        for x in seq:
            h = ((h ^ x) * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return str(h)


rows = []
for path in sorted(glob.glob(os.path.join(ROOT, "extracted", "pak", "*.WAD"))):
    d = open(path, "rb").read()
    if d[:4] != b"\x15\0\0\0":
        continue
    s = find(d)
    if not s:
        continue
    polys = s["polys"]
    tris = sum(1 for q in polys if len(q[1]) == 3)
    names = "|".join(f"{n}:{f:08x}:{h:08x}" for n, f, h in s["surfaces"])
    # normals are compared by class (floats differ in the last digit between f32 and f64)
    up = sum(1 for q in polys if q[2][1] > 0.5)
    down = sum(1 for q in polys if q[2][1] < -0.5)
    rows.append((os.path.basename(path)[:-4], len(s["verts"]), tris, len(polys) - tris, len(s["surfaces"]), len(s["flag_names"]),
                 names, checksum(polys), f"{up}/{down}"))
with open(sys.argv[1], "w", encoding="utf-8", newline="") as f:
    f.write("wad\tverts\ttris\tquads\tsurfaces\tflagnames\tsurface_list\tcheck\tup_down\n")
    for r in rows:
        f.write("\t".join(str(x) for x in r) + "\n")
print(len(rows), "levels")
