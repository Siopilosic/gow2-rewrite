"""Textured preview of one model: verifies UV scale and the part -> material-slot rule.

  python tools/textured_preview.py extracted/pak/RHOD10.WAD innerPillar out.png [--uvscale 4096]

Material slots = the MAT_ reference records between `MDL_<n>` (88 B) and `MDL_<n>_0` in the WAD
(observed layout, docs/models.md). Part material slot = low 16 bits of part header word +0x08.
Texture lookup MAT -> TXR -> GFX/PAL (tools/gfx_decode.py). Inspection renderer, not PS2 shading.
"""
import argparse
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from gfx_decode import TextureStore  # noqa: E402
from mdl_decode import batches, parts, write_png  # noqa: E402


def model_materials(recs_seq, name):
    out, inside = [], False
    for tag, n, body in recs_seq:
        if tag == 1 and n == f"MDL_{name}" and len(body) == 88:
            inside = True
            continue
        if inside:
            if n == f"MDL_{name}_0":
                return out
            if tag == 1 and n.startswith("MAT_") and not body:
                out.append(n)
    return out


def mesh_full(blob, uvscale):
    V, UV, COL, T = [], [], [], []  # T: (a, b, c, slot)
    for (i, j, k, C, kind, pk) in parts(blob):
        slot = struct.unpack_from("<I", blob, C + 8)[0] & 0xFFFF
        for g, s, e in pk:
            if g != 0:
                continue
            for b in batches(blob, s, e):
                if "pos" not in b:
                    continue
                base = len(V)
                p = b["pos"]
                V += [(v[0], v[1], v[2]) for v in p]
                UV += [(u / uvscale, v / uvscale) for u, v in b.get("uv", [(0, 0)] * len(p))]
                COL += b.get("col", [(128, 128, 128, 128)] * len(p))
                for n in range(2, len(p)):
                    if not (p[n][3] & 0x8000):
                        t = (base + n - 2, base + n - 1, base + n)
                        t = t if n % 2 == 0 else (t[1], t[0], t[2])
                        T.append((*t, slot))
    return V, UV, COL, T


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("name")
    ap.add_argument("out")
    ap.add_argument("--uvscale", type=float, default=4096.0)
    ap.add_argument("--size", type=int, default=640)
    ap.add_argument("--yaw", type=float, default=0.6)
    ap.add_argument("--pitch", type=float, default=0.25)
    a = ap.parse_args()
    data = open(a.wad, "rb").read()
    seq, recs = [], {}
    for off, tag, param, n, body in wad_records(data):
        seq.append((tag, n, body))
        if tag == 1 and body:
            recs.setdefault(n, body)
    mats = model_materials(seq, a.name)
    store = TextureStore(recs)
    tex = [store.material_texture(m) for m in mats]
    print("materials:", [(m, (t[0], t[1]) if t else None) for m, t in zip(mats, tex)])
    V, UV, COL, T = mesh_full(recs[f"MDL_{a.name}_0"], a.uvscale)
    us = [u for u, v in UV]
    print(f"{len(V)} verts {len(T)} tris; u range {min(us):.3f}..{max(us):.3f}")
    S = a.size
    cy, sy, cp, sp = math.cos(a.yaw), math.sin(a.yaw), math.cos(a.pitch), math.sin(a.pitch)
    tv = []
    for x, y, z in V:
        x1, z1 = x * cy + z * sy, -x * sy + z * cy
        tv.append((x1, y * cp - z1 * sp, y * sp + z1 * cp))
    xs, ys = [v[0] for v in tv], [v[1] for v in tv]
    sc = (S - 20) / max(max(xs) - min(xs), max(ys) - min(ys), 1e-6)
    scr = [((v[0] - min(xs)) * sc + 10, S - ((v[1] - min(ys)) * sc + 10), v[2]) for v in tv]
    zb = [-1e30] * (S * S)
    img = bytearray(b"\x20\x20\x28" * (S * S))
    for a_, b_, c_, slot in T:
        A, B, Cc = scr[a_], scr[b_], scr[c_]
        den = (B[1] - Cc[1]) * (A[0] - Cc[0]) + (Cc[0] - B[0]) * (A[1] - Cc[1])
        if abs(den) < 1e-9:
            continue
        t = tex[slot] if slot < len(tex) else None
        x0, x1 = int(max(0, min(A[0], B[0], Cc[0]))), int(min(S - 1, max(A[0], B[0], Cc[0])))
        y0, y1 = int(max(0, min(A[1], B[1], Cc[1]))), int(min(S - 1, max(A[1], B[1], Cc[1])))
        for py in range(y0, y1 + 1):
            for px in range(x0, x1 + 1):
                l1 = ((B[1] - Cc[1]) * (px - Cc[0]) + (Cc[0] - B[0]) * (py - Cc[1])) / den
                l2 = ((Cc[1] - A[1]) * (px - Cc[0]) + (A[0] - Cc[0]) * (py - Cc[1])) / den
                l3 = 1 - l1 - l2
                if l1 < 0 or l2 < 0 or l3 < 0:
                    continue
                z = l1 * A[2] + l2 * B[2] + l3 * Cc[2]
                k = py * S + px
                if z <= zb[k]:
                    continue
                zb[k] = z
                col = [l1 * COL[a_][q] + l2 * COL[b_][q] + l3 * COL[c_][q] for q in range(3)]
                if t:
                    w, h, rgba = t
                    u = l1 * UV[a_][0] + l2 * UV[b_][0] + l3 * UV[c_][0]
                    v = l1 * UV[a_][1] + l2 * UV[b_][1] + l3 * UV[c_][1]
                    tx, ty = int(u * w) % w, int(v * h) % h
                    o = (ty * w + tx) * 4
                    rgb = [min(255, int(rgba[o + q] * col[q] / 128)) for q in range(3)]
                else:
                    rgb = [min(255, int(col[q] * 2)) for q in range(3)]
                img[k * 3:k * 3 + 3] = bytes(rgb)
    write_png(a.out, S, S, img)
    print("wrote", a.out)


if __name__ == "__main__":
    main()
