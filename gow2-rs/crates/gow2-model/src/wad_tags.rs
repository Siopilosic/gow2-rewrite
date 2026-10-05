//! WAD tag handler table registered by `Wad_InitLoader` (0x0018cc00) into
//! `g_WadTagHandlers` (0x0036a6b8, 0x17 entries) via `Wad_SetTagHandler` (0x0018d988).
//! CONFIRMED (handler addresses). Behaviour is recorded only where it was traced.

#[derive(Debug, Clone, Copy)]
pub struct TagHandler {
    pub tag: u16,
    pub handler: u32,
    pub known: &'static str,
}

const fn h(tag: u16, handler: u32, known: &'static str) -> TagHandler {
    TagHandler { tag, handler, known }
}

pub const HANDLERS: &[TagHandler] = &[
    h(0x00, 0x0018_d7c0, "TODO: not traced (named-integer records on disc)"),
    h(0x01, 0x0018_5748, "Object: g_ServerTable[type&0xffff]->vslot10(hdr,data); name registration; group link"),
    h(0x02, 0x0018_5968, "GroupStart: g_WadGroupPending = 1"),
    h(0x03, 0x0018_5978, "GroupEnd: owner(parent)->vslot11(parent, hdr); pop group stack"),
    h(0x04, 0x0018_ce20, "TODO: not traced (tag never seen on disc)"),
    h(0x05, 0x0018_59f0, "ActivateByName: obj=lookup(name); r=owner(obj)->vslot5(obj); owner(r)->vslot16(r)"),
    h(0x06, 0x0018_5a78, "TODO: not traced ('PopContext' on disc)"),
    h(0x07, 0x0018_d7f8, "TODO: not traced (MC_DATA / MC_ICONSYS)"),
    h(0x08, 0x0018_d870, "TODO: not traced (MSGS_TXT)"),
    h(0x09, 0x0018_d8f0, "TODO: not traced (MSH_* shapes)"),
    h(0x0b, 0x0012_0588, "TODO: not traced (DC_*)"),
    h(0x0c, 0x0012_05e8, "TODO: not traced (DC_*)"),
    h(0x0d, 0x0012_0698, "TODO: not traced (DC_*)"),
    h(0x0e, 0x0012_07b8, "TODO: not traced (DC_*)"),
    h(0x0f, 0x0012_0898, "TODO: not traced (DC_*; stream loader may skip oversized payloads)"),
    h(0x10, 0x0012_08b8, "TODO: not traced (DC_*; stream loader may skip oversized payloads)"),
    h(0x11, 0x0015_9c78, "TODO: not traced (tag never seen on disc)"),
    h(0x13, 0x0018_59f0, "same handler as tag 5 (HeaderEnd record named after the WAD)"),
    h(0x14, 0x0018_5a78, "same handler as tag 6"),
    h(0x15, 0x0018_cb68, "TODO: not traced (first record of every WAD)"),
    h(0x16, 0x0018_cbe0, "TODO: not traced ('PopHeap' on disc)"),
];

/// Tags 0x0a and 0x12 have no handler: `Wad_DispatchRecord` skips the call when the
/// table entry is null (`beqz v0` at 0x001890b8) but still advances by the stride.
pub const UNHANDLED: &[u16] = &[0x0a, 0x12];

pub fn handler(tag: u16) -> Option<&'static TagHandler> {
    HANDLERS.iter().find(|h| h.tag == tag)
}

/// Record stride computed by `Wad_DispatchRecord`: `((size + 15) & !15) + 0x20`.
pub const fn dispatch_stride(size: u32) -> u32 {
    ((size + 15) & !15) + 0x20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stride_matches_parser() {
        // The parser in gow2-formats aligns header+payload; the engine aligns the
        // payload only. They agree because the header is 0x20 bytes.
        for size in [0u32, 1, 8, 15, 16, 17, 48, 88, 1_162_384] {
            assert_eq!(dispatch_stride(size) as usize, (0x20 + size as usize + 15) & !15);
        }
    }

    #[test]
    fn table_fits_handler_array() {
        assert!(HANDLERS.iter().all(|h| h.tag < 0x17));
    }
}
