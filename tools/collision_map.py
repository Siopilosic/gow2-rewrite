"""Top-down PNG of a level's collision sheet: floors green (shaded by height), walls orange, ceilings blue, other surfaces grey.

  python tools/collision_map.py <level.WAD> <out.png> [x,z ...]     # extra x,z pairs are marked with red crosses

No imaging library needed (writes the PNG with zlib). Use it to see where a level is walkable and where the walls are.
"""
import os
import struct
import sys
import zlib

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from sheet_decode import find  # noqa: E402

FLAG_SKIP = 0x41010042  # the walking sweep's skip mask (docs/collision.md)


def png(path, img):
    h, w, _ = img.shape
    raw = b"".join(b"\0" + img[y].tobytes() for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def main():
    s = find(open(sys.argv[1], "rb").read())
    args = [a for a in sys.argv[3:] if not a.startswith("--")]
    opts = {a.split("=")[0]: a.split("=")[1] for a in sys.argv[3:] if a.startswith("--")}
    marks = [tuple(map(float, a.split(","))) for a in args]
    (bx0, by0, bz0), (bx1, by1, bz1) = s["bounds"]
    # --crop=x0,z0,x1,z1 zooms in; --y=lo,hi keeps only polygons that reach into that height band (walls the body can hit)
    if "--crop" in opts:
        bx0, bz0, bx1, bz1 = map(float, opts["--crop"].split(","))
    ylo, yhi = map(float, opts["--y"].split(",")) if "--y" in opts else (-1e9, 1e9)
    size = 1400 if "--crop" not in opts else 1000
    scale = size / max(bx1 - bx0, bz1 - bz0)
    W, H = int((bx1 - bx0) * scale) + 20, int((bz1 - bz0) * scale) + 20
    img = np.full((H, W, 3), 20, np.uint8)
    zbuf = np.full((H, W), -1e9, np.float32)

    def to_px(v):
        return ((v[0] - bx0) * scale + 10, (v[2] - bz0) * scale + 10)

    order = sorted(range(len(s["polys"])), key=lambda i: sum(v[1] for v in s["polys"][i][1]) / len(s["polys"][i][1]))
    for i in order:
        surf, corners, n = s["polys"][i]
        if max(v[1] for v in corners) < ylo or min(v[1] for v in corners) > yhi:
            continue
        name, flags, _ = s["surfaces"][surf]
        if abs(n[1]) > 0.7:
            col = np.array([40, 160, 60]) if n[1] > 0 else np.array([60, 90, 200])
            t = (sum(v[1] for v in corners) / len(corners) - by0) / max(by1 - by0, 1)
            col = (col * (0.45 + 0.55 * t)).astype(np.uint8)
        else:
            col = np.array([230, 130, 30], np.uint8)
        if "--hl" in opts and opts["--hl"] in name:
            col = np.array([255, 0, 255], np.uint8)
        elif flags & 0x41510042 and abs(n[1]) <= 0.7:
            col = np.array([0, 190, 190], np.uint8)  # a wall the hero skips (guides, NoPlayerCollision)
        elif flags & 0x41010042 and abs(n[1]) > 0.7:
            col = np.array([110, 110, 110], np.uint8)
        tris = [corners[:3]] + ([[corners[0], corners[2], corners[3]]] if len(corners) == 4 else [])
        for tri in tris:
            p = [to_px(v) for v in tri]
            xs, ys = [q[0] for q in p], [q[1] for q in p]
            x0, x1, y0, y1 = int(min(xs)), int(max(xs)) + 1, int(min(ys)), int(max(ys)) + 1
            if x1 <= x0 or y1 <= y0:
                # a vertical wall seen from above is a line
                x0, x1, y0, y1 = max(x0 - 1, 0), min(x1 + 1, W), max(y0 - 1, 0), min(y1 + 1, H)
            gx, gy = np.meshgrid(np.arange(max(x0, 0), min(x1, W)) + 0.5, np.arange(max(y0, 0), min(y1, H)) + 0.5)
            (ax, ay), (bx, by), (cx, cy) = p
            d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(d) < 1e-9:
                # degenerate in plan: draw as a polyline
                for (qx, qy) in p:
                    if 0 <= int(qx) < W and 0 <= int(qy) < H:
                        img[int(qy), int(qx)] = col
                continue
            l1 = ((by - cy) * (gx - cx) + (cx - bx) * (gy - cy)) / d
            l2 = ((cy - ay) * (gx - cx) + (ax - cx) * (gy - cy)) / d
            inside = (l1 >= -0.02) & (l2 >= -0.02) & (1 - l1 - l2 >= -0.02)
            ys_, xs_ = np.nonzero(inside)
            img[ys_ + max(y0, 0), xs_ + max(x0, 0)] = col
    for (mx, mz) in marks:
        px, py = to_px((mx, 0, mz))
        for k in range(-8, 9):
            for (xx, yy) in ((int(px) + k, int(py) + k), (int(px) + k, int(py) - k)):
                if 0 <= xx < W and 0 <= yy < H:
                    img[yy, xx] = (255, 0, 0)
    png(sys.argv[2], img)
    print("bounds", s["bounds"], "image", W, "x", H, "px, scale", round(scale, 3))


main()



