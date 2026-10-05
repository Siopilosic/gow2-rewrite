"""Survey all MAT_ records (120 bytes) across level WADs; correlate field values with name hints
(ADD / SUBTRACT / ALPHA / GLOW ...) to decode blend fields. Research tool, output TSV.

  python tools/mat_survey.py extracted/pak analysis/materials/mat_survey.tsv
"""
import collections
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from gfx_decode import cstr  # noqa: E402


def main(src, out):
    rows, seen = [], set()
    for fn in sorted(os.listdir(src)):
        if not fn.upper().endswith(".WAD"):
            continue
        data = open(os.path.join(src, fn), "rb").read()
        recs = {}
        for _o, tag, _p, n, b in wad_records(data):
            if tag == 1 and b:
                recs.setdefault(n, b)
        for n, b in recs.items():
            if not n.startswith("MAT_") or len(b) != 120:
                continue
            key = (n, b)
            if key in seen:
                continue
            seen.add(key)
            w = struct.unpack_from("<30I", b)
            txr = cstr(b, 0x48)
            t = recs.get(txr)
            gfx = cstr(t, 4) if t and len(t) >= 0x34 else ""
            g = recs.get(gfx)
            bpp = struct.unpack_from("<I", g, 16)[0] if g and len(g) >= 24 else -1
            rows.append([fn[:-4], n, txr, str(bpp)] + [f"{x:08x}" for x in w])
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        f.write("\t".join(["wad", "name", "txr", "bpp"] + [f"w{i * 4:02x}" for i in range(30)]) + "\n")
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(len(rows), "unique MAT records")
    # per word: distinct values
    for i in range(30):
        c = collections.Counter(r[4 + i] for r in rows)
        if len(c) > 1:
            print(f"w{i * 4:02x}", len(c), c.most_common(6))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
