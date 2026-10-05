//! Object-record payload layouts that were confirmed from both sides: the
//! executable's reader and every matching record on the disc.

use crate::{fixed_str, le_u32};

/// Server-instance record payload (type word bit 31 set), 48 bytes.
/// Writer in the executable: `BootWad_BeginInstance` (0x00189870).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstancePayload {
    pub type_word: u32,
    pub field_04: u32,
    pub field_08: u32,
    pub field_0c: u32,
    pub field_10: u32,
    pub field_14: u32,
    pub name: String,
}

pub const INSTANCE_PAYLOAD_LEN: usize = 0x30;

pub fn parse_instance(p: &[u8]) -> Option<InstancePayload> {
    if p.len() < INSTANCE_PAYLOAD_LEN || le_u32(p, 0) & 0x8000_0000 == 0 {
        return None;
    }
    Some(InstancePayload {
        type_word: le_u32(p, 0),
        field_04: le_u32(p, 4),
        field_08: le_u32(p, 8),
        field_0c: le_u32(p, 0xc),
        field_10: le_u32(p, 0x10),
        field_14: le_u32(p, 0x14),
        name: fixed_str(&p[0x18..0x30]),
    })
}

/// `TXR_` texture record payload, 88 bytes, read by `Texture_ctor` (0x001745d8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxrPayload {
    pub gfx_name: String,
    pub pal_name: String,
    pub ref3_name: String,
    pub field_4c: u32,
    pub field_50: u32,
    pub field_54: u32,
}

pub const TXR_PAYLOAD_LEN: usize = 0x58;

pub fn parse_txr(p: &[u8]) -> Option<TxrPayload> {
    if p.len() != TXR_PAYLOAD_LEN || le_u32(p, 0) != 7 {
        return None;
    }
    Some(TxrPayload {
        gfx_name: fixed_str(&p[0x04..0x1c]),
        pal_name: fixed_str(&p[0x1c..0x34]),
        ref3_name: fixed_str(&p[0x34..0x4c]),
        field_4c: le_u32(p, 0x4c),
        field_50: le_u32(p, 0x50),
        field_54: le_u32(p, 0x54),
    })
}
