"""Decode MDL_ model parts into vertex batches; export OBJ and a preview PNG (research decoder).

  python tools/mdl_decode.py <WAD> <MDL_name_0> --dump PART
  python tools/mdl_decode.py <WAD> <MDL_name_0> --obj out.obj [--png out.png]

Hierarchy: docs/models.md (confirmed from FUN_00168168). Each DMA `ref` packet holds a sequence of
vertex *batches*, triple-buffered in VU1 memory (bases 0x000/0x155/0x2ab, +TOPS). One batch is:
    UNPACK V2-16 (UV)  V3-8 (normal)  V4-16 (position, w = flags)  V4-8u (colour)  V4-32 (header)
Attribute roles are hypotheses tested by the previews; strip rule: w bit 15 = no triangle (ADC).
"""
import argparse
import math
import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402

FMT = {0x0: (1, "i", 4), 0x1: (1, "h", 2), 0x2: (1, "b", 1), 0x4: (2, "i", 4), 0x5: (2, "h", 2),
       0x6: (2, "b", 1), 0x8: (3, "i", 4), 0x9: (3, "h", 2), 0xA: (3, "b", 1), 0xC: (4, "i", 4),
       0xD: (4, "h", 2), 0xE: (4, "b", 1), 0xF: (1, "H", 2)}
ROLE = {0x5: "uv", 0xA: "nrm", 0xD: "pos", 0xE: "col", 0xC: "hdr"}


def batches(buf, pos, end):
    """Yield batches (dict role -> list of tuples) in stream order; a V4-32 header closes a batch."""
    cur = {}
    while pos + 4 <= end:
        code = struct.unpack_from("<I", buf, pos)[0]
        pos += 4
        cmd, num, imm = (code >> 24) & 0x7F, (code >> 16) & 0xFF, code & 0xFFFF
        if cmd >= 0x60:
            fmt = cmd & 0xF
            comps, ch, sz = FMT[fmt]
            if imm & 0x4000:
                ch = ch.upper()
            n = num or 256
            vals = [struct.unpack_from("<" + ch * comps, buf, pos + i * comps * sz) for i in range(n)]
            pos += (n * comps * sz + 3) & ~3
            role = ROLE.get(fmt, f"f{fmt:x}")
            cur[role] = vals
            if role == "hdr":
                yield cur
                cur = {}
        elif cmd == 0x20:
            pos += 4
        elif cmd in (0x30, 0x31):
            pos += 16
        elif cmd == 0x4A:
            pos += (num or 256) * 8
        elif cmd in (0x50, 0x51):
            pos += (imm or 65536) * 16
    if cur:
        yield cur


def parts(blob):
    u16 = lambda o: struct.unpack_from("<H", blob, o)[0]  # noqa: E731
    u32 = lambda o: struct.unpack_from("<I", blob, o)[0]  # noqa: E731
    for i in range(u16(8)):
        A = u32(0x18 + 4 * i)
        for j in range(u16(A + 2)):
            Bo = A + u32(A + 4 + 4 * j)
            for k in range(u16(Bo + 4)):
                C = Bo + u32(Bo + 8 + 4 * k)
                kind = struct.unpack_from("<h", blob, C)[0]
                pk = []
                if kind in (0x18, 0x0E):
                    groups, per = blob[C + 0x18] * u32(C + 0xC), u32(C + 4)
                    e = C + 0x20
                    for g in range(groups):
                        for q in range(per):
                            w0, addr = struct.unpack_from("<II", blob, e)
                            if (w0 >> 28) & 7 in (0, 3, 4):
                                pk.append((g, C + addr, C + addr + (w0 & 0xFFFF) * 16))
                            e += 16
                yield (i, j, k, C, kind, pk)


def mesh(blob, group=0):
    """Vertices, colours, triangles of DMA group `group` (groups: LODs or matrix sets, unknown)."""
    verts, cols, tris = [], [], []
    for (i, j, k, C, kind, pk) in parts(blob):
        for g, s, e in pk:
            if g != group:
                continue
            for b in batches(blob, s, e):
                if "pos" not in b:
                    continue
                base = len(verts)
                p = b["pos"]
                verts += [(v[0], v[1], v[2]) for v in p]
                cols += b.get("col", [(128, 128, 128, 128)] * len(p))
                for n in range(2, len(p)):
                    if not (p[n][3] & 0x8000):
                        t = (base + n - 2, base + n - 1, base + n)
                        tris.append(t if n % 2 == 0 else (t[1], t[0], t[2]))
    return verts, cols, tris


def write_png(path, w, h, rgb):
    raw = b"".join(b"\0" + bytes(rgb[y * w * 3:(y + 1) * w * 3]) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))  # noqa: E731
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def render(verts, cols, tris, path, size=512, yaw=0.6, pitch=0.25):
    """Orthographic z-buffered preview, face shading x vertex colour. Inspection only, not PS2 shading."""
    cy, sy, cp, sp = math.cos(yaw), math.sin(yaw), math.cos(pitch), math.sin(pitch)
    tv = []
    for x, y, z in verts:
        x1, z1 = x * cy + z * sy, -x * sy + z * cy
        y1, z2 = y * cp - z1 * sp, y * sp + z1 * cp
        tv.append((x1, y1, z2))
    xs, ys = [v[0] for v in tv], [v[1] for v in tv]
    mnx, mxx, mny, mxy = min(xs), max(xs), min(ys), max(ys)
    sc = (size - 20) / max(mxx - mnx, mxy - mny, 1e-6)
    scr = [((v[0] - mnx) * sc + 10, size - ((v[1] - mny) * sc + 10), v[2]) for v in tv]
    zb = [-1e30] * (size * size)
    img = bytearray(b"\x20\x20\x28" * (size * size))
    for a, b, c in tris:
        A, B, C = scr[a], scr[b], scr[c]
        ux, uy, uz = tv[b][0] - tv[a][0], tv[b][1] - tv[a][1], tv[b][2] - tv[a][2]
        vx, vy, vz = tv[c][0] - tv[a][0], tv[c][1] - tv[a][1], tv[c][2] - tv[a][2]
        nx, ny, nz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
        ln = math.sqrt(nx * nx + ny * ny + nz * nz) or 1
        shade = 0.35 + 0.65 * abs(nz / ln)
        col = cols[a]
        rgb = bytes(min(255, int(min(col[q], 128) * 2 * shade)) for q in range(3))
        x0, x1 = int(max(0, min(A[0], B[0], C[0]))), int(min(size - 1, max(A[0], B[0], C[0])))
        y0, y1 = int(max(0, min(A[1], B[1], C[1]))), int(min(size - 1, max(A[1], B[1], C[1])))
        den = (B[1] - C[1]) * (A[0] - C[0]) + (C[0] - B[0]) * (A[1] - C[1])
        if abs(den) < 1e-9:
            continue
        for py in range(y0, y1 + 1):
            for px in range(x0, x1 + 1):
                l1 = ((B[1] - C[1]) * (px - C[0]) + (C[0] - B[0]) * (py - C[1])) / den
                l2 = ((C[1] - A[1]) * (px - C[0]) + (A[0] - C[0]) * (py - C[1])) / den
                l3 = 1 - l1 - l2
                if l1 < 0 or l2 < 0 or l3 < 0:
                    continue
                z = l1 * A[2] + l2 * B[2] + l3 * C[2]
                k = py * size + px
                if z > zb[k]:
                    zb[k] = z
                    img[k * 3:k * 3 + 3] = rgb
    write_png(path, size, size, img)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("name")
    ap.add_argument("--dump", type=int)
    ap.add_argument("--obj")
    ap.add_argument("--png")
    ap.add_argument("--group", type=int, default=0)
    a = ap.parse_args()
    data = open(a.wad, "rb").read()
    blob = next(body for off, tag, param, nm, body in wad_records(data) if tag == 1 and nm == a.name)
    if a.dump is not None:
        i, j, k, C, kind, pk = list(parts(blob))[a.dump]
        print(f"part {a.dump} @C={C:#x} kind={kind:#x} dma packets={len(pk)} groups={sorted({g for g, _, _ in pk})}")
        for g, s, e in pk[:1]:
            for bi, b in enumerate(batches(blob, s, e)):
                print(f"  batch {bi}: " + ", ".join(f"{r}={len(v)}" for r, v in b.items()))
                if bi == 0:
                    for r in ("pos", "uv", "nrm", "col"):
                        print(f"     {r}: {b.get(r, [])[:4]}")
        return
    verts, cols, tris = mesh(blob, a.group)
    xs, ys, zs = zip(*verts)
    print(f"{a.name}: {len(verts)} verts, {len(tris)} tris, bbox x[{min(xs)},{max(xs)}] "
          f"y[{min(ys)},{max(ys)}] z[{min(zs)},{max(zs)}]")
    if a.obj:
        with open(a.obj, "w") as f:
            for v, c in zip(verts, cols):
                f.write(f"v {v[0]} {v[1]} {v[2]} {min(c[0], 128) / 128:.3f} {min(c[1], 128) / 128:.3f} "
                        f"{min(c[2], 128) / 128:.3f}\n")
            for t in tris:
                f.write(f"f {t[0] + 1} {t[1] + 1} {t[2] + 1}\n")
    if a.png:
        render(verts, cols, tris, a.png)
        print("preview", a.png)


if __name__ == "__main__":
    main()
