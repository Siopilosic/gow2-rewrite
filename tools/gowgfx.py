"""Decode GFX_/PAL_ image records from a WAD to PNG (no third-party deps).

GFX payload: u32 type(=0x0C) u32 width u32 height u32 encoding u32 bpp u32 count, pixels...
PAL payload: same header, bpp=32, width*height = colour count, RGBA8888 with PS2 alpha
(0x80 = opaque). 8-bit CLUTs are stored in the GS CSM1 layout (blocks of 8 swapped).

Usage: python gowgfx.py <file.wad> <out-dir> [name-substring]
"""
import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(__file__))
from gowwad import iter_tags  # noqa: E402


def write_png(path, w, h, rgba):
    raw = b"".join(b"\0" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def parse_image(p):
    typ, w, h, enc, bpp, count = struct.unpack_from("<6I", p)
    return w, h, enc, bpp, count, p[24:]


def csm1_unswizzle(pal):
    if len(pal) != 256:
        return pal
    out = list(pal)
    for i in range(256):
        j = (i & ~0x18) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)
        out[i] = pal[j]
    return out


def decode(gfx, pal_rec, unswizzle_clut=True):
    w, h, enc, bpp, count, px = parse_image(gfx)
    pw, ph, _, _, _, pd = parse_image(pal_rec)
    pal = [pd[i * 4:i * 4 + 4] for i in range(pw * ph)]
    if unswizzle_clut:
        pal = csm1_unswizzle(pal)
    pal = [bytes((c[0], c[1], c[2], min(255, c[3] * 2))) for c in pal]
    out = bytearray()
    if bpp == 8:
        for i in range(w * h):
            out += pal[px[i]]
    elif bpp == 4:
        for i in range(w * h):
            b = px[i >> 1]
            out += pal[(b >> 4) if i & 1 else (b & 0xF)]
    else:
        raise ValueError(f"bpp {bpp} not handled yet")
    return w, h, bytes(out)


def main():
    data = open(sys.argv[1], "rb").read()
    out = sys.argv[2]
    filt = sys.argv[3] if len(sys.argv) > 3 else ""
    os.makedirs(out, exist_ok=True)
    gfx, pal = {}, {}
    for off, tag, param, size, name, p in iter_tags(data, True):
        if tag == 1 and len(p) >= 24 and struct.unpack_from("<I", p)[0] == 0x0C:
            (pal if name.startswith("PAL_") else gfx)[name[4:]] = p
    n = 0
    for key, g in gfx.items():
        if filt not in key or key not in pal:
            continue
        try:
            w, h, rgba = decode(g, pal[key])
        except ValueError as e:
            print(f"skip {key}: {e}")
            continue
        enc = parse_image(g)[2]
        write_png(os.path.join(out, f"{key}_enc{enc}.png"), w, h, rgba)
        n += 1
    print(f"wrote {n} images")


if __name__ == "__main__":
    main()
