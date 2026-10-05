//! `GODOFWAR.TOC` and the `PART1.PAK` / `PART2.PAK` archives.
//!
//! ```text
//! u32 file_count
//! file_count x { char name[24]; u32 size; u32 copy_count; u32 first_copy; }
//! u32 copy_sector[]   // rest of file
//! ```
//! A file may be stored several times (in either PAK) to cut seek times; all
//! copies are byte-identical. Sector values `< 10_000_000` address PART1.PAK,
//! values `>= 10_000_000` address PART2.PAK at `value - 10_000_000`.

use std::io;

use crate::iso::{Entry, Iso};
use crate::{fixed_str, le_u32};

pub const PART2_BASE: u32 = 10_000_000;

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub name: String,
    pub size: u32,
    /// Sector values as stored in the TOC (see module docs for decoding).
    pub copies: Vec<u32>,
}

pub fn parse_toc(data: &[u8]) -> Vec<TocEntry> {
    let count = le_u32(data, 0) as usize;
    let tbl = 4 + count * 36;
    let sectors: Vec<u32> = data[tbl..].chunks_exact(4).map(|c| le_u32(c, 0)).collect();
    (0..count)
        .map(|i| {
            let off = 4 + i * 36;
            let ncopies = le_u32(data, off + 28) as usize;
            let first = le_u32(data, off + 32) as usize;
            TocEntry {
                name: fixed_str(&data[off..off + 24]),
                size: le_u32(data, off + 24),
                copies: sectors[first..first + ncopies].to_vec(),
            }
        })
        .collect()
}

/// Random access into both PAKs of a disc image.
pub struct Pak {
    pub iso: Iso,
    parts: [(usize, Entry); 2],
}

impl Pak {
    pub fn open(mut iso: Iso) -> io::Result<Self> {
        let p1 = iso.find("PART1.PAK")?;
        let p2 = iso.find("PART2.PAK")?;
        Ok(Self { iso, parts: [p1, p2] })
    }

    pub fn read(&mut self, sector: u32, size: u32) -> io::Result<Vec<u8>> {
        let (part, rel) = if sector >= PART2_BASE { (1, sector - PART2_BASE) } else { (0, sector) };
        let (vol, e) = &self.parts[part];
        if rel as u64 * crate::iso::SECTOR + size as u64 > e.size as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "read past end of PAK"));
        }
        let base = self.iso.volumes[*vol].base + e.lba as u64;
        self.iso.read_at(base + rel as u64, size as usize)
    }

    pub fn read_entry(&mut self, e: &TocEntry) -> io::Result<Vec<u8>> {
        self.read(e.copies[0], e.size)
    }
}
