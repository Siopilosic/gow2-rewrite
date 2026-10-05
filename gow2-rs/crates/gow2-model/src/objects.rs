//! Recovered in-memory layouts (offsets only; this crate never builds these objects).
//! Field names follow `analysis/structs.txt`: `field_XX` unless evidence supports a name.

/// gcc 2.x (non-thunk) vtable entry. CONFIRMED by every virtual call site:
/// `lh delta, N(vt); lw fn, N+4(vt); addu a0, obj, delta; jalr fn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GccVtableEntry {
    pub delta: i16,
    pub index: i16,
    pub fn_addr: u32,
}

impl GccVtableEntry {
    pub fn parse(b: &[u8]) -> Self {
        Self {
            delta: i16::from_le_bytes([b[0], b[1]]),
            index: i16::from_le_bytes([b[2], b[3]]),
            fn_addr: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        }
    }
}

/// Byte offset of the vtable pointer in every polymorphic engine class recovered so far.
pub const VPTR_OFFSET: u32 = 0x20;
/// Byte offset of virtual slot `n` inside a vtable (entry 0 is the null entry).
pub const fn vslot_offset(n: u32) -> u32 {
    n * 8
}

/// Common header (`ObjectHeader`, 0x10 bytes) shared by all objects built with the
/// `(word0 & 0x0fffffff) | 0x40000000` idiom, including non-polymorphic ones (textures).
pub mod header {
    /// u32: record type word, masked, with bit 30 set. Low 16 bits = owning server.
    pub const TYPE_WORD: u32 = 0x00;
    /// u16: initialised to 0x20.
    pub const FLAGS: u32 = 0x04;
    pub const FIELD_06: u32 = 0x06;
    /// List node (next, prev) used when the object is a child in a manager list.
    pub const LINK_NEXT: u32 = 0x08;
    pub const LINK_PREV: u32 = 0x0c;
    /// Present only in `PolyObject` (base-class constructor idiom).
    pub const ORDER_KEY: u32 = 0x10;
    pub const POOL_INDEX: u32 = 0x1c;

    /// Value every constructor stores into `flags` first.
    pub const FLAGS_INITIAL: u16 = 0x20;
    /// Set by `Mgr_AddChildSorted`/`Bank_AddChild`, cleared by `Mgr_RemoveChild`.
    pub const FLAG_LINKED: u16 = 0x04;
    /// `Mgr_UpdateChildren` skips children with this bit.
    pub const FLAG_NO_UPDATE: u16 = 0x10;
    /// `Mgr_UpdateChildren` calls Master vslot 8 = `Mgr_RemoveChild` (0x0027f9d0 ->
    /// 0x00273190) instead of updating when any of these is set: mark-for-removal.
    /// TODO(0x00273190): who sets them; bit 1 additionally makes the remover call vslot 9.
    pub const FLAG_REMOVE_MASK: u16 = 0x03;
}

/// `type_word` as built by constructors from a record's first payload word.
pub const fn object_type_word(record_word0: u32) -> u32 {
    (record_word0 & 0x0fff_ffff) | 0x4000_0000
}

/// Pooled-server layout (`PooledServer`, 0xd8 bytes; WadServer 0xe0, GOServer 0x11c).
pub mod pooled_server {
    pub const BUCKETS: u32 = 0x24;
    pub const BUCKET_COUNT: u32 = 0x40;
    pub const CUR_BUCKET: u32 = 0x44;
    /// Context stack (`ctx_stack[ctx_top]` is the current bank/context).
    pub const CTX_STACK: u32 = 0x48;
    /// s8, -1 when empty.
    pub const CTX_TOP: u32 = 0xc8;
    pub const DEFAULT_BANK_INDEX: u32 = 0xd4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_word_masking() {
        // Master_Server boot record (5<<16)|0 -> object type word 0x40050000.
        assert_eq!(object_type_word(5 << 16), 0x4005_0000);
        // Instance record bit 31 is dropped by the mask; bit 30 is always set.
        assert_eq!(object_type_word(0x8000_0007), 0x4000_0007);
    }

    #[test]
    fn vslot_offsets_match_call_sites() {
        // WadTag_Object calls `lh a0,0x50(vt); lw v0,0x54(vt)` = slot 10.
        assert_eq!(vslot_offset(10), 0x50);
        // Engine_UpdateServers calls Master slot 3 at 0x18/0x1c.
        assert_eq!(vslot_offset(3), 0x18);
    }
}
