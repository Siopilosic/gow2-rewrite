//! ISO9660 reader for PS2 DVD images, including dual-layer (DVD9) images where
//! layer 1 carries its own volume descriptor.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::{fixed_str, le_u32};

pub const SECTOR: u64 = 2048;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    /// Sector relative to the owning volume.
    pub lba: u32,
    pub size: u32,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Volume {
    /// Absolute sector where this volume starts (0 for layer 0).
    pub base: u64,
    pub blocks: u32,
    root_lba: u32,
    root_size: u32,
}

pub struct Iso {
    file: File,
    pub volumes: Vec<Volume>,
}

impl Iso {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let total = file.metadata()?.len() / SECTOR;
        let l0 = read_volume(&mut file, 0)?;
        let mut volumes = vec![l0];
        // Layer 1's PVD sits 16 sectors after (layer0 size - 16).
        let base = l0.blocks as u64 - 16;
        if base + 17 < total {
            if let Ok(l1) = read_volume(&mut file, base) {
                volumes.push(l1);
            }
        }
        Ok(Self { file, volumes })
    }

    pub fn read_at(&mut self, abs_sector: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; len];
        self.file.seek(SeekFrom::Start(abs_sector * SECTOR))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Lists every entry of a volume, depth first.
    pub fn walk(&mut self, vol: usize) -> io::Result<Vec<Entry>> {
        let v = self.volumes[vol];
        let mut out = Vec::new();
        self.walk_dir(&v, v.root_lba, v.root_size, "", &mut out)?;
        Ok(out)
    }

    fn walk_dir(&mut self, v: &Volume, lba: u32, size: u32, parent: &str, out: &mut Vec<Entry>) -> io::Result<()> {
        let data = self.read_at(v.base + lba as u64, size as usize)?;
        let mut off = 0usize;
        while off < data.len() {
            let len = data[off] as usize;
            if len == 0 {
                // Records never straddle sectors.
                off = (off / SECTOR as usize + 1) * SECTOR as usize;
                continue;
            }
            let e_lba = le_u32(&data, off + 2);
            let e_size = le_u32(&data, off + 10);
            let is_dir = data[off + 25] & 2 != 0;
            let nlen = data[off + 32] as usize;
            let raw = &data[off + 33..off + 33 + nlen];
            if raw != [0] && raw != [1] {
                let name = fixed_str(raw);
                let name = name.split(';').next().unwrap_or("").to_string();
                let path = if parent.is_empty() { name } else { format!("{parent}/{name}") };
                out.push(Entry { path: path.clone(), lba: e_lba, size: e_size, is_dir });
                if is_dir {
                    self.walk_dir(v, e_lba, e_size, &path, out)?;
                }
            }
            off += len;
        }
        Ok(())
    }

    /// Finds a file on any layer; returns (volume index, entry).
    pub fn find(&mut self, path: &str) -> io::Result<(usize, Entry)> {
        let want = path.trim_matches('/').to_ascii_uppercase();
        for v in 0..self.volumes.len() {
            if let Some(e) = self.walk(v)?.into_iter().find(|e| e.path.to_ascii_uppercase() == want) {
                return Ok((v, e));
            }
        }
        Err(io::Error::new(io::ErrorKind::NotFound, path.to_string()))
    }

    pub fn read_file(&mut self, vol: usize, e: &Entry) -> io::Result<Vec<u8>> {
        let base = self.volumes[vol].base;
        self.read_at(base + e.lba as u64, e.size as usize)
    }
}

fn read_volume(file: &mut File, base: u64) -> io::Result<Volume> {
    let mut pvd = vec![0u8; SECTOR as usize];
    file.seek(SeekFrom::Start((base + 16) * SECTOR))?;
    file.read_exact(&mut pvd)?;
    if &pvd[1..6] != b"CD001" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no ISO9660 volume descriptor"));
    }
    Ok(Volume {
        base,
        blocks: le_u32(&pvd, 80),
        root_lba: le_u32(&pvd, 156 + 2),
        root_size: le_u32(&pvd, 156 + 10),
    })
}
