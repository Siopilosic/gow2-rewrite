"""GFX_/PAL_/TXR_/MAT_ helpers in Python (same rules as gow2-rs crates/gow2-formats/src/gfx.rs).

GFX/PAL: u32 type(0x0C) w h encoding bpp count; data. PAL = RGBA8888, alpha 0x80 = opaque,
256-entry palettes in CSM1 order. TXR (88 bytes): +4 gfx name[24], +0x1c pal name[24].
MAT (120 bytes): +0x48 TXR name[24] (observed in RHOD10; docs/models.md).
"""
import struct


def csm1(i):
    return (i & ~0x18) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)


def unswizzle8(buf, w, h):
    """GS PSMT8 -> linear: 8-bit indices stored in the PSMCT32 block/column layout.
    Required for RHOD10's 8-bpp level textures (docs/models.md); standard PS2 algorithm."""
    out = bytearray(w * h)
    for y in range(h):
        for x in range(w):
            block = (y & ~0xF) * w + (x & ~0xF) * 2
            swap = (((y + 2) >> 2) & 1) * 4
            posy = (((y & ~3) >> 1) + (y & 1)) & 7
            col = posy * w * 2 + ((x + swap) & 7) * 4
            byte = ((y >> 1) & 1) + ((x >> 2) & 2)
            out[y * w + x] = buf[block + col + byte]
    return bytes(out)


def cstr(b, o, n=24):
    return b[o:o + n].split(b"\0", 1)[0].decode("latin-1")


def decode(gfx, pal):
    """Return (w, h, rgba bytes) or None when the combination is not understood."""
    if len(gfx) < 24 or len(pal) < 24:
        return None
    w, h, enc, bpp = struct.unpack_from("<4I", gfx, 4)
    pw, ph, penc, pbpp = struct.unpack_from("<4I", pal, 4)
    ncol = pw * ph
    if pbpp != 32 or bpp not in (4, 8) or ncol < (1 << bpp) or len(pal) < 24 + ncol * 4:
        return None
    pd, gd = pal[24:], gfx[24:]
    # PSMT8-swizzled (docs/formats.md, R13); enc 2 = linear. Sizes below one 16x16 block
    # (e.g. RHOD20 GFX_IronStrap01 8x32, data = w*h) are read linear: LOW, not verified.
    if bpp == 8 and enc == 0 and w % 16 == 0 and h % 16 == 0 and len(gd) >= w * h:
        gd = unswizzle8(gd[:w * h], w, h)
    lut = []
    for i in range(1 << bpp):
        # CSM1 order applies to every 8-bpp palette of 256 or more entries (Kratos's has 512: two sets); the old
        # rule (ncol == 256) left those unremapped and gave salt-and-pepper noise (checked on MAT_kratos1A, 2026-10-03)
        j = csm1(i) if bpp == 8 and ncol >= 256 else i
        r, g, b, a = pd[j * 4:j * 4 + 4]
        lut.append(bytes((r, g, b, min(255, a * 2))))
    out = bytearray()
    for i in range(w * h):
        if bpp == 8:
            idx = gd[i]
        else:
            byte = gd[i >> 1]
            idx = (byte >> 4) if i & 1 else (byte & 0xF)
        out += lut[idx]
    return w, h, bytes(out)


class TextureStore:
    """Resolve MAT name -> decoded texture through MAT -> TXR -> GFX/PAL records of one WAD."""

    def __init__(self, records):
        self.r = records  # name -> payload (first non-empty record of that name)
        self.cache = {}

    def material_texture(self, mat_name):
        if mat_name in self.cache:
            return self.cache[mat_name]
        res = None
        mat = self.r.get(mat_name)
        if mat and len(mat) >= 0x60:
            txr = self.r.get(cstr(mat, 0x48))
            if txr and len(txr) >= 0x34:
                g, p = self.r.get(cstr(txr, 4)), self.r.get(cstr(txr, 0x1C))
                if g and p:
                    res = decode(g, p)
        self.cache[mat_name] = res
        return res
