//! Material to texture resolution: MAT -> TXR -> GFX/PAL records of one WAD (port of `TextureStore` in
//! `tools/gfx_decode.py`). MAT record (>= 0x60 bytes): TXR name at +0x48. TXR: GFX name at +4, PAL name at +0x1c.

use std::collections::HashMap;

use crate::{fixed_str, gfx};

/// A decoded texture: width, height and RGBA8888 pixels.
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Name -> payload of the first non-empty record with that name.
pub struct TextureStore<'a> {
    records: HashMap<String, &'a [u8]>,
}

impl<'a> TextureStore<'a> {
    pub fn new(records: impl IntoIterator<Item = (String, &'a [u8])>) -> Self {
        let mut map: HashMap<String, &'a [u8]> = HashMap::new();
        for (n, d) in records {
            if !d.is_empty() {
                map.entry(n).or_insert(d);
            }
        }
        Self { records: map }
    }

    /// A texture by its `TXR_<name>` record, without a material (UI sprites: `TXR_HUDA010` -> `GFX_HUDA010` + `PAL_HUDA010`).
    pub fn txr_texture(&self, txr_name: &str) -> Option<Texture> {
        let txr = self.records.get(txr_name)?;
        if txr.len() < 0x34 {
            return None;
        }
        let g = self.records.get(fixed_str(&txr[4..0x1c]).as_str())?;
        let p = self.records.get(fixed_str(&txr[0x1c..0x34]).as_str())?;
        let (gi, pi) = (gfx::parse(g)?, gfx::parse(p)?);
        let rgba = gfx::to_rgba(&gi, &pi)?;
        Some(Texture { width: gi.width, height: gi.height, rgba })
    }

    pub fn material_texture(&self, mat_name: &str) -> Option<Texture> {
        let mat = self.records.get(mat_name)?;
        if mat.len() < 0x60 {
            return None;
        }
        let txr = self.records.get(fixed_str(&mat[0x48..0x60]).as_str())?;
        if txr.len() < 0x34 {
            return None;
        }
        let g = self.records.get(fixed_str(&txr[4..0x1c]).as_str())?;
        let p = self.records.get(fixed_str(&txr[0x1c..0x34]).as_str())?;
        let (gi, pi) = (gfx::parse(g)?, gfx::parse(p)?);
        let rgba = gfx::to_rgba(&gi, &pi)?;
        Some(Texture { width: gi.width, height: gi.height, rgba })
    }
}

