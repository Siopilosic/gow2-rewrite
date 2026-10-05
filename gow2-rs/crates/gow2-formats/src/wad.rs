//! `.WAD` level/resource containers.
//!
//! A WAD is a flat stream of 16-byte-aligned records:
//! ```text
//! struct Record { u16 tag; u16 param; u32 size; char name[24]; u8 data[size]; }
//! ```
//! Confirmed from both sides:
//! * header layout: written field by field by `BootWad_BeginRecord` (0x00189160);
//! * stride `0x20 + ((size + 15) & !15)`: returned by `Wad_DispatchRecord` (0x0018d6d8);
//! * tag 0 carries no payload: `Wad_StreamUpdate` sends tag 0 to
//!   `Wad_DispatchRecordNoPayload` without reading data (0x0018d10c..0x0018d130);
//! * all 292 standard WADs on the disc parse exactly to EOF with these rules.
//!
//! Tag handlers are listed in `gow2_model::wad_tags`. The `Tag` variant names below
//! are labels taken from record names on the disc, not from the executable.

use crate::{fixed_str, le_u16, le_u32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Value,
    Object,
    GroupStart,
    GroupEnd,
    HeaderEnd,
    WadHeader,
    PopHeap,
    Other(u16),
}

impl From<u16> for Tag {
    fn from(t: u16) -> Self {
        match t {
            0x00 => Tag::Value,
            0x01 => Tag::Object,
            0x02 => Tag::GroupStart,
            0x03 => Tag::GroupEnd,
            0x13 => Tag::HeaderEnd,
            0x15 => Tag::WadHeader,
            0x16 => Tag::PopHeap,
            t => Tag::Other(t),
        }
    }
}

/// Engine server ids, from the low 16 bits of an object's type word.
/// Names come from the WAD instance prefixes and the server name strings in
/// the executable (`TextureServer`, `MatServer`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ServerId {
    Context = 0x01,       // CXT_ / go* game objects
    Anim = 0x03,          // ANM_
    Script = 0x04,        // SCR_
    Light = 0x06,         // LGT_
    Texture = 0x07,       // TXR_
    Material = 0x08,      // MAT_
    Camera = 0x09,        // CAM_
    GfxClut = 0x0C,       // GFX_ / PAL_
    Model = 0x0F,         // MDL_
    Collision = 0x10,     // COL_ / CDV_ / CDZ_
    Particle = 0x11,      // PRT_ / PTC_
    Waypoint = 0x12,      // WYP_
    Behavior = 0x14,      // BHV_
    Sound = 0x15,         // SND_ / SBP_ / SEM_
    Wad = 0x16,           // WAD_
    EePrim = 0x17,        // EEPR_
    Effects = 0x19,       // FX_ / FXC_
    Flash = 0x1B,         // FLP_
    ShadowGeom = 0x20,    // SHG_
}

impl ServerId {
    pub fn from_u16(v: u16) -> Option<Self> {
        use ServerId::*;
        Some(match v {
            0x01 => Context,
            0x03 => Anim,
            0x04 => Script,
            0x06 => Light,
            0x07 => Texture,
            0x08 => Material,
            0x09 => Camera,
            0x0C => GfxClut,
            0x0F => Model,
            0x10 => Collision,
            0x11 => Particle,
            0x12 => Waypoint,
            0x14 => Behavior,
            0x15 => Sound,
            0x16 => Wad,
            0x17 => EePrim,
            0x19 => Effects,
            0x1B => Flash,
            0x20 => ShadowGeom,
            _ => return None,
        })
    }
}

/// Decoded object type word: `[31] instance flag | [30:16] subtype | [15:0] server`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjType(pub u32);

impl ObjType {
    pub fn is_server_instance(self) -> bool {
        self.0 & 0x8000_0000 != 0
    }
    pub fn server(self) -> Option<ServerId> {
        ServerId::from_u16(self.0 as u16)
    }
    pub fn subtype(self) -> u16 {
        ((self.0 >> 16) & 0x7FFF) as u16
    }
}

#[derive(Debug, Clone)]
pub struct Record<'a> {
    pub offset: usize,
    pub tag: Tag,
    pub param: u16,
    /// Payload length, or the value itself for [`Tag::Value`].
    pub size: u32,
    pub name: String,
    pub data: &'a [u8],
}

impl Record<'_> {
    pub fn obj_type(&self) -> Option<ObjType> {
        (self.tag == Tag::Object && self.data.len() >= 4).then(|| ObjType(le_u32(self.data, 0)))
    }
}

pub struct Records<'a> {
    data: &'a [u8],
    off: usize,
}

pub fn records(data: &[u8]) -> Records<'_> {
    Records { data, off: 0 }
}

impl<'a> Iterator for Records<'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Record<'a>> {
        let d = self.data;
        let off = self.off;
        if off + 32 > d.len() {
            return None;
        }
        let tag = Tag::from(le_u16(d, off));
        let size = le_u32(d, off + 4);
        let body = if tag == Tag::Value { 0 } else { size as usize };
        let end = (off + 32 + body).min(d.len());
        self.off = (off + 32 + body + 15) & !15;
        Some(Record {
            offset: off,
            tag,
            param: le_u16(d, off + 2),
            size,
            name: fixed_str(&d[off + 8..off + 32]),
            data: &d[off + 32..end],
        })
    }
}

/// The name under which a model's mesh record is stored: `MDL_<model>_0`, but a name longer than 19 characters keeps only its last 19
/// (`MDL_storageRoomMain_0` is stored as `L_storageRoomMain_0`; the engine builds it in a 20-byte buffer). Material, texture and other
/// records keep names up to 23 characters.
pub fn mesh_record_name(model: &str) -> String {
    let full = format!("MDL_{model}_0");
    if full.len() > 19 {
        full[full.len() - 19..].to_string()
    } else {
        full
    }
}
