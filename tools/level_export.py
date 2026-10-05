"""Export every MDL_*_0 model of a level WAD into one OBJ (+ overview PNG). Research tool.

  python tools/level_export.py extracted/pak/RHOD10.WAD analysis/levels/RHOD10 [--png]

Geometry only (positions, vertex colours, strips) as decoded by tools/mdl_decode.py. Model data
is used as stored: environment models were observed to be in world coordinates already
(docs/models.md). Instanced models placed by go* nodes are NOT positioned yet.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from mdl_decode import mesh, render  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("out")
    ap.add_argument("--png", action="store_true")
    ap.add_argument("--exclude", default="", help="comma-separated substrings of model names to skip")
    ap.add_argument("--yaw", type=float, default=0.0)
    ap.add_argument("--pitch", type=float, default=1.2)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    data = open(a.wad, "rb").read()
    allv, allc, allt, report = [], [], [], []
    with open(os.path.join(a.out, "level.obj"), "w") as f:
        for off, tag, param, name, body in wad_records(data):
            if tag != 1 or not name.startswith("MDL_") or not name.endswith("_0") or len(body) < 64:
                continue
            if any(x and x.lower() in name.lower() for x in a.exclude.split(",")):
                continue
            try:
                v, c, t = mesh(body)
            except Exception as e:  # noqa: BLE001 - recorded, not hidden
                report.append({"model": name, "error": repr(e)})
                continue
            if not v:
                report.append({"model": name, "verts": 0})
                continue
            xs, ys, zs = zip(*v)
            report.append({"model": name, "verts": len(v), "tris": len(t),
                           "bbox": [min(xs), min(ys), min(zs), max(xs), max(ys), max(zs)]})
            base = len(allv)
            f.write(f"o {name}\n")
            for p, col in zip(v, c):
                f.write(f"v {p[0]} {p[1]} {p[2]} {min(col[0], 128) / 128:.3f} {min(col[1], 128) / 128:.3f} "
                        f"{min(col[2], 128) / 128:.3f}\n")
            for tri in t:
                f.write(f"f {tri[0] + base + 1} {tri[1] + base + 1} {tri[2] + base + 1}\n")
            allv += v
            allc += c
            allt += [(x + base, y + base, z + base) for x, y, z in t]
    json.dump(report, open(os.path.join(a.out, "models.json"), "w"), indent=1)
    ok = [r for r in report if r.get("verts")]
    print(f"{len(ok)} models, {len(allv)} verts, {len(allt)} tris; errors {sum('error' in r for r in report)}")
    if a.png:
        render(allv, allc, allt, os.path.join(a.out, "overview.png"), size=768, yaw=a.yaw, pitch=a.pitch)
        print("overview written")


if __name__ == "__main__":
    main()
