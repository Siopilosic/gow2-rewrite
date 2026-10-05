//! Record -> server routing rules and a static simulation of the WAD tag handlers'
//! bookkeeping (group stack), used to validate the model against disc data.

use gow2_formats::wad::{self, Tag};

/// `g_ServerTable` (0x00362c48) has 256 entries (`ServerTable_Init` clears 0..0x100).
pub const SERVER_TABLE_LEN: usize = 256;

/// Server that receives an object record: `g_ServerTable[type_word & 0xffff]`
/// (`WadTag_Object`, 0x00185748). CONFIRMED.
pub const fn owner_server(type_word: u32) -> u16 {
    type_word as u16
}

/// Index a server object registers itself under: `(type_word >> 16) & 0xfff`
/// (`Server_RegisterSelf` 0x00277118, `TextureServer_Init` 0x00288b10). CONFIRMED.
pub const fn self_index(type_word: u32) -> u16 {
    ((type_word >> 16) & 0xfff) as u16
}

/// Instance records (bit 31) select the server's default bank in
/// `PooledServer_SelectBank` (0x00288f40). CONFIRMED.
pub const fn is_instance_record(type_word: u32) -> bool {
    type_word & 0x8000_0000 != 0
}

/// Subtype used by the factories and by `GoClassA4_CreateFromRecord`.
pub const fn subtype(type_word: u32) -> u16 {
    ((type_word & 0x0fff_ffff) >> 16) as u16
}

/// Depth of `g_WadGroupStack` (0x00366010): `g_WadGroupTop` (0x0036600c) starts at
/// 0x10, meaning empty. A pending GroupStart pushes with a pre-decrement and **no
/// bounds check** (0x00185924..0x00185940); `WadTag_Object` links a new object to
/// `stack[top]` only while `top < 0x10` (0x001858dc). Disc maximum nesting is 3.
pub const GROUP_STACK_DEPTH: usize = 16;

#[derive(Debug, Default, Clone)]
pub struct RouteReport {
    pub objects_per_server: std::collections::BTreeMap<u16, usize>,
    pub instance_records: usize,
    pub name_only_objects: usize,
    pub max_group_depth: usize,
    /// GroupStart seen while a previous GroupStart was still pending (never on disc).
    pub group_start_while_pending: usize,
    /// Pending GroupStart consumed by something other than an Object record.
    pub group_start_not_followed_by_object: usize,
    pub unknown_servers: Vec<(String, u32)>,
}

/// Replays the tag handlers' bookkeeping for one WAD:
/// GroupStart sets a pending flag (0x00185968); the next Object becomes the group
/// parent (0x00185914..); GroupEnd pops (0x00185978).
pub fn route(data: &[u8]) -> RouteReport {
    let mut rep = RouteReport::default();
    let (mut depth, mut pending) = (0usize, false);
    for rec in wad::records(data) {
        match rec.tag {
            Tag::GroupStart => {
                if pending {
                    rep.group_start_while_pending += 1;
                }
                pending = true;
                continue;
            }
            Tag::GroupEnd => depth = depth.saturating_sub(1),
            Tag::Object => {
                if rec.size == 0 {
                    rep.name_only_objects += 1;
                } else if let Some(t) = rec.obj_type() {
                    let srv = owner_server(t.0);
                    if crate::servers::class(srv).is_none() {
                        rep.unknown_servers.push((rec.name.clone(), t.0));
                    }
                    if is_instance_record(t.0) {
                        rep.instance_records += 1;
                    }
                    *rep.objects_per_server.entry(srv).or_default() += 1;
                }
                if pending {
                    pending = false;
                    depth += 1;
                    rep.max_group_depth = rep.max_group_depth.max(depth);
                }
                continue;
            }
            _ => {}
        }
        if pending {
            rep.group_start_not_followed_by_object += 1;
            pending = false;
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_rules() {
        assert_eq!(owner_server(0x0000_0007), 7); // TXR_ record -> TextureServer
        assert_eq!(owner_server(0x0016_0005), 5); // WadServer boot record -> Master
        assert_eq!(self_index(0x4016_0005), 0x16); // WadServer registers itself at 0x16
        assert!(is_instance_record(0x8000_000c));
        assert_eq!(subtype(0x0003_0001), 3); // gochest
    }
}
