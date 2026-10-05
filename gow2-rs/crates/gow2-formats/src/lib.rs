//! Readers for God of War II (PS2, SCUS-97481) disc and asset formats.
//!
//! Every layout here was verified against the retail US disc; see
//! `docs/formats.md` in the project root for the write-ups.

pub mod anm;
pub mod cdv;
pub mod dc;
pub mod flp;
pub mod flp_play;
pub mod gfx;
pub mod iso;
pub mod level;
pub mod mdl;
pub mod records;
pub mod sheet;
pub mod skin;
pub mod snd;
pub mod texture;
pub mod toc;
pub mod wad;

pub(crate) fn le_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

pub(crate) fn le_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Reads a NUL-terminated name from a fixed-size field.
pub(crate) fn fixed_str(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

pub(crate) fn le_f32(b: &[u8], off: usize) -> f32 {
    f32::from_bits(le_u32(b, off))
}

/// Public wrapper for tools and examples.
pub fn le_u32_pub(b: &[u8], off: usize) -> u32 {
    le_u32(b, off)
}


