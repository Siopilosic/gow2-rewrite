"""Oracle summaries for the Rust texture resolver: MAT_ name -> (width, height, FNV hash of the RGBA pixels).

  python tools/oracle_tex.py gow2-rs/crates/gow2-formats/tests/oracle/tex.tsv

Uses tools/gfx_decode.py TextureStore on first non-empty records by name (tag 1), as ptc_export.py does.
A material that does not decode is written with width 0. Only derived numbers are stored.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from gfx_decode import TextureStore  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WADS = ["R_HERO00", "R_HERO01", "R_ZEUS", "RHOD10", "R_PERMA", "R_SHELLA"]
M = 0xFFFFFFFFFFFFFFFF

with open(sys.argv[1], "w", encoding="utf-8", newline="") as out:
    out.write("wad\tmaterial\twidth\theight\tcheck\n")
    for w in WADS:
        p = os.path.join(ROOT, "extracted", "pak", w + ".WAD")
        if not os.path.exists(p):
            continue
        recs = {}
        for _o, tag, _p, n, body in wad_records(open(p, "rb").read()):
            if tag == 1 and body:
                recs.setdefault(n, body)
        store = TextureStore(recs)
        for n in recs:
            if not n.startswith("MAT_"):
                continue
            t = store.material_texture(n)
            if t is None:
                out.write(f"{w}\t{n}\t0\t0\t0\n")
                continue
            tw, th, rgba = t
            h = 1469598103934665603
            for x in rgba:
                h = ((h ^ x) * 1099511628211) & M
            out.write(f"{w}\t{n}\t{tw}\t{th}\t{h}\n")