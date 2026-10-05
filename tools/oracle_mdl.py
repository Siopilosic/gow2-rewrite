"""Oracle summaries for the Rust MDL decoder: counts and a checksum per WAD/model/group.

  python tools/oracle_mdl.py gow2-rs/crates/gow2-formats/tests/oracle/mdl.tsv

The Rust test (tests/mdl_oracle.rs) must reproduce every number. Only derived counts are stored, no game data.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from mdl_decode import mesh  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WADS = ["R_HERO00", "R_HERO01", "R_ZEUS", "R_MINTAR00", "RHOD10"]


def checksum(verts, cols, tris):
    h = 1469598103934665603
    for seq in (verts, cols, tris):
        for t in seq:
            for x in t:
                h = ((h ^ (int(x) & 0xFFFFFFFFFFFFFFFF)) * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return str(h)


out = []
for w in WADS:
    p = os.path.join(ROOT, "extracted", "pak", w + ".WAD")
    if not os.path.exists(p):
        continue
    d = open(p, "rb").read()
    for _o, tag, _p, n, body in wad_records(d):
        if tag == 1 and n.startswith("MDL_") and body and len(body) > 0x100:
            try:
                for g in (0, 1):
                    v, c, t = mesh(body, g)
                    out.append({"wad": w, "name": n, "group": g, "verts": len(v), "tris": len(t),
                                "check": checksum(v, c, t)})
            except Exception:
                pass
with open(sys.argv[1], "w", encoding="utf-8", newline="") as f:
    f.write("wad\tname\tgroup\tverts\ttris\tcheck\n")
    for r in out:
        f.write(f"{r['wad']}\t{r['name']}\t{r['group']}\t{r['verts']}\t{r['tris']}\t{r['check']}\n")
