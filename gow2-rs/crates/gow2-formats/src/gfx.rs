//! `GFX_` (indexed pixels) and `PAL_` (palette) objects, server id 0x0C.
//!
//! ```text
//! u32 type (=0x0C) u32 width u32 height u32 encoding u32 bpp u32 count; data...
//! ```
//! Palettes are RGBA8888 with PS2 alpha (0x80 = opaque); 256-colour palettes
//! are stored in GS CSM1 order. Pixel data is linear (verified for encoding 0
//! and 2 at 4 and 8 bpp).

use crate::le_u32;

#[derive(Debug, Clone)]
pub struct Image<'a> {
    pub width: u32,
    pub height: u32,
    pub encoding: u32,
    pub bpp: u32,
    pub count: u32,
    pub data: &'a [u8],
}

pub fn parse(p: &[u8]) -> Option<Image<'_>> {
    if p.len() < 24 || le_u32(p, 0) != 0x0C {
        return None;
    }
    Some(Image {
        width: le_u32(p, 4),
        height: le_u32(p, 8),
        encoding: le_u32(p, 12),
        bpp: le_u32(p, 16),
        count: le_u32(p, 20),
        data: &p[24..],
    })
}

/// Maps a CSM1-ordered CLUT index to its linear position.
fn csm1(i: usize) -> usize {
    (i & !0x18) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)
}

/// GS PSMT8 -> linear (8-bit indices stored in PSMCT32 block/column order).
pub fn unswizzle8(buf: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let block = (y & !0xF) * w + (x & !0xF) * 2;
            let swap = (((y + 2) >> 2) & 1) * 4;
            let posy = (((y & !3) >> 1) + (y & 1)) & 7;
            let col = posy * w * 2 + ((x + swap) & 7) * 4;
            let byte = ((y >> 1) & 1) + ((x >> 2) & 2);
            let src = block + col + byte;
            if src < buf.len() {
                out[y * w + x] = buf[src];
            }
        }
    }
    out
}

/// Expands an indexed image to RGBA8888 using `pal`.
///
/// Returns `None` for combinations not yet understood (e.g. an 8-bpp image whose indices exceed
/// a 16-colour palette, seen in RHOD10) instead of guessing a mapping.
pub fn to_rgba(img: &Image, pal: &Image) -> Option<Vec<u8>> {
    if pal.bpp != 32 {
        return None;
    }
    let ncol = (pal.width * pal.height) as usize;
    if pal.data.len() < ncol * 4 || ncol < (1usize << img.bpp.min(8)) {
        return None;
    }
    let colour = |idx: usize| -> [u8; 4] {
        // CSM1 order for every 8-bpp palette of 256 or more entries (Kratos's has 512)
        let i = if img.bpp == 8 && ncol >= 256 { csm1(idx) } else { idx };
        let c = &pal.data[i * 4..i * 4 + 4];
        [c[0], c[1], c[2], (c[3] as u16 * 2).min(255) as u8]
    };
    let n = (img.width * img.height) as usize;
    // 8-bpp with encoding 0 is stored PSMT8-swizzled; encoding 2 is linear
    // (checked visually on RHOD10, R_SHELLA and R_PERMA textures; docs/formats.md).
    // sizes below one 16x16 block are read linear (LOW, not verified; matches tools/gfx_decode.py)
    if img.bpp == 8 && img.data.len() < n || img.bpp == 4 && img.data.len() < n.div_ceil(2) {
        return None;
    }
    let unswz = if img.bpp == 8 && img.encoding == 0 && img.width % 16 == 0 && img.height % 16 == 0 {
        Some(unswizzle8(&img.data[..n], img.width as usize, img.height as usize))
    } else {
        None
    };
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let idx = match img.bpp {
            8 => unswz.as_ref().map_or(img.data[i], |u| u[i]) as usize,
            4 => {
                let b = img.data[i >> 1];
                (if i & 1 == 1 { b >> 4 } else { b & 0xF }) as usize
            }
            _ => return None,
        };
        out.extend_from_slice(&colour(idx));
    }
    Some(out)
}
