//! Server registrations and server classes.
//!
//! `REGISTRATIONS` is transcribed from `Boot_CreateServersAndEngineResources`
//! (0x00189b10): each entry is one `BootWad_AddServer4/6` call, which emits a boot
//! record with type word `(id << 16) | parent` and payload
//! `{type, 0, arg_a3, arg_t0, arg_t1[, arg_t2, arg_t3]}` (CONFIRMED).
//!
//! `CLASSES` is transcribed from the two server factories, `Master_NewServer`
//! (0x00185be0) and `RenMaster_NewServer` (0x00160bc8), plus the constructor/vtable
//! chain found by `tools/server_profile.py` (CONFIRMED).

/// One `BootWad_AddServer*` call. The `arg_*` names are the MIPS EABI registers the
/// value was passed in; their meaning is recorded only where it is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registration {
    pub name: &'static str,
    /// Server that constructs this one (the low 16 bits of the boot record's type word).
    pub parent: u16,
    pub id: u16,
    /// Becomes header `field_18`; for pooled servers it sizes `field_34` (u32 array).
    pub arg_a3: u32,
    /// Becomes header `field_14`; for pooled servers it is the per-bucket capacity.
    pub arg_t0: u32,
    /// Becomes header `order_key` (+0x10). CONFIRMED: Master keeps its children
    /// sorted by this value, descending (`Mgr_AddChildSorted`, 0x00272fd8), and
    /// `Mgr_UpdateChildren` (0x00277200) visits them in that order every frame.
    pub order_key: u32,
    /// Only for `BootWad_AddServer6`. Becomes `PooledServer::field_cc` / `field_d0`.
    pub arg_t2: Option<u32>,
    pub arg_t3: Option<u32>,
}

const fn r4(name: &'static str, parent: u16, id: u16, a3: u32, t0: u32, t1: u32) -> Registration {
    Registration { name, parent, id, arg_a3: a3, arg_t0: t0, order_key: t1, arg_t2: None, arg_t3: None }
}

#[allow(clippy::too_many_arguments)]
const fn r6(name: &'static str, parent: u16, id: u16, a3: u32, t0: u32, t1: u32, t2: u32, t3: u32) -> Registration {
    Registration { name, parent, id, arg_a3: a3, arg_t0: t0, order_key: t1, arg_t2: Some(t2), arg_t3: Some(t3) }
}

/// In call order, exactly as at 0x00189b2c..0x00189f0c.
pub const REGISTRATIONS: &[Registration] = &[
    r4("Master_Server", 0x00, 0x05, 100, 100, 0x7fff),
    r6("WadServer", 0x05, 0x16, 0x80, 0x80, 0x7f0f, 0, 0),
    r4("ProServer", 0x05, 0x0a, 0, 0, 0x7fff),
    r6("GOServer", 0x05, 0x01, 0x200, 0x200, 0x5f00, 0x80, 0x80),
    r6("AnimServer", 0x05, 0x03, 0x80, 0x80, 0x7100, 0x100, 0x100),
    r6("BhvrServer", 0x05, 0x14, 0x80, 0x80, 0x7002, 0x82, 0x82),
    r4("EvtServer", 0x05, 0x13, 0, 0, 0x7001),
    r6("CollisionServer", 0x05, 0x10, 0x80, 0x80, 0x6200, 200, 200),
    r6("ScriptServer", 0x05, 0x04, 0x80, 0x80, 0x6000, 0x400, 0x400),
    r6("CameraServer", 0x05, 0x09, 0x80, 0x80, 0x5e00, 0x40, 0x40),
    r6("WaypointServer", 0x05, 0x12, 0x80, 0x80, 0x5e00, 0x40, 0x40),
    r6("EffectsServer", 0x05, 0x19, 0x80, 0x80, 0x5e00, 0x200, 0x200),
    r6("LightServer", 0x05, 0x06, 100, 100, 0x5d00, 0x80, 0x80),
    r6("MatServer", 0x05, 0x08, 0x200, 0x200, 0x5c00, 0x200, 0x200),
    r6("TextureServer", 0x05, 0x07, 0x80, 0x80, 0x5b00, 0x200, 0x200),
    r6("GfxClutServer", 0x05, 0x0c, 0x80, 0x80, 0x5a00, 0x200, 0x200),
    r6("SoundServer", 0x05, 0x15, 0x80, 0x80, 0x5901, 1000, 1000),
    r6("renMasterSvr", 0x05, 0x0d, 100, 100, 0x5900, 0x8000, 0x4b000),
    r6("renModelServer", 0x0d, 0x0f, 0x80, 0x80, 0x1000, 500, 500),
    r6("renParticleSvr", 0x0d, 0x11, 0x80, 0x80, 0x1000, 1000, 1000),
    r6("renFlashServer", 0x0d, 0x1b, 0x80, 0x80, 0x1000, 500, 500),
    r6("renShadowServer", 0x0d, 0x20, 0x80, 0x80, 0x1000, 0x40, 0x40),
    r6("renEEPrimSvr", 0x0d, 0x17, 0x80, 0x80, 0x1000, 500, 500),
    r4("renPrimMaster", 0x05, 0x02, 0x80, 0x80, 0x5700),
    r4("EpiServer", 0x05, 0x0b, 0, 0, 0),
];

/// How a server object is built by its parent's factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerClass {
    pub id: u16,
    /// Factory that builds it: 0x00185be0 (Master) or 0x00160bc8 (renMaster).
    pub factory: u32,
    /// Bytes passed to `Mem_New`.
    pub size: u32,
    /// Out-of-line constructor, or `None` when the factory constructs inline.
    pub ctor: Option<u32>,
    /// Most-derived vtable (gcc 2.x layout, see `objects::GccVtableEntry`).
    pub vtable: u32,
    /// True when the class derives from the pooled-server base (vtable 0x002f68b0).
    pub pooled: bool,
}

const MASTER: u32 = 0x0018_5be0;
const RENMASTER: u32 = 0x0016_0bc8;

const fn c(id: u16, factory: u32, size: u32, ctor: Option<u32>, vtable: u32, pooled: bool) -> ServerClass {
    ServerClass { id, factory, size, ctor, vtable, pooled }
}

/// Root object (server table slot 0) is built by `ServerTable_Init` (0x00186f18):
/// 0x38 bytes, vtable 0x002f6968. Master_Server is built by the root's slot 19
/// (`Root_NewMasterServer`, 0x00187048) - the Master factory has an identical case 5.
pub const CLASSES: &[ServerClass] = &[
    c(0x01, MASTER, 0x11c, Some(0x0014_2888), 0x002f_6470, true),
    c(0x02, MASTER, 0x16c, Some(0x0016_d740), 0x002f_44e8, false), // TODO(0x0016d740): installs 3 vtables, class shape unverified
    c(0x03, MASTER, 0x4dc, None, 0x002f_5e28, true),
    c(0x04, MASTER, 0xd8, Some(0x0013_fe48), 0x002f_5298, true),
    c(0x05, MASTER, 0x34, None, 0x002f_4808, false),
    c(0x06, MASTER, 0xd8, None, 0x002f_26b0, true),
    c(0x07, MASTER, 0xd8, None, 0x002f_2930, true),
    c(0x08, MASTER, 0xd8, Some(0x0016_2c08), 0x002f_3d48, true),
    c(0x09, MASTER, 0xd8, None, 0x002f_2ad0, true),
    c(0x0a, MASTER, 0x30, None, 0x002f_1fa0, false),
    c(0x0b, MASTER, 0x2c, None, 0x002f_48e8, false),
    c(0x0c, MASTER, 0xd8, Some(0x0015_b328), 0x002f_3fc8, true),
    c(0x0d, MASTER, 0x4c, Some(0x0016_0a68), 0x002f_4558, false),
    c(0x0f, RENMASTER, 0xd8, Some(0x0016_54d0), 0x002f_2d50, true),
    c(0x10, MASTER, 0xd8, Some(0x0011_f240), 0x002f_4a30, true),
    c(0x11, RENMASTER, 0xd8, Some(0x0016_bf58), 0x002f_3768, true),
    c(0x12, MASTER, 0xd8, Some(0x0018_f908), 0x002f_1d20, true),
    c(0x13, MASTER, 0x3834, Some(0x0012_1788), 0x002f_5178, false),
    c(0x14, MASTER, 0xd8, Some(0x0011_e280), 0x002f_4e30, true),
    c(0x15, MASTER, 0xd8, Some(0x0018_0188), 0x002f_2080, true),
    c(0x16, MASTER, 0xe0, None, 0x002f_6110, true),
    c(0x17, RENMASTER, 0xd8, Some(0x0014_fb90), 0x002f_4248, true),
    c(0x19, MASTER, 0xd8, Some(0x0013_8518), 0x002f_3218, true),
    // Both factories have a case for 0x1b. The boot record names renMasterSvr (0x0d) as
    // parent, so RenMaster_NewServer is the one that runs at boot.
    c(0x1b, RENMASTER, 0xd8, Some(0x0015_8d90), 0x002f_3aa8, true),
    c(0x20, RENMASTER, 0xd8, Some(0x0017_3310), 0x002f_2410, true),
];

pub fn registration(id: u16) -> Option<&'static Registration> {
    REGISTRATIONS.iter().find(|r| r.id == id)
}

pub fn class(id: u16) -> Option<&'static ServerClass> {
    CLASSES.iter().find(|c| c.id == id)
}

/// The order `Mgr_UpdateChildren` visits Master's direct children each frame:
/// descending `order_key`. Ties keep insertion order only if `Mgr_AddChildSorted`
/// inserts equal keys after existing ones.
/// TODO(0x00272fd8): confirm tie placement; the cursor-based walk makes it non-obvious.
pub fn master_update_order() -> Vec<&'static Registration> {
    let mut v: Vec<_> = REGISTRATIONS.iter().filter(|r| r.parent == 0x05).collect();
    v.sort_by(|a, b| b.order_key.cmp(&a.order_key));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registration_has_a_class_and_parent() {
        for r in REGISTRATIONS {
            assert!(class(r.id).is_some(), "{} has no factory case", r.name);
            if r.parent != 0 {
                assert!(registration(r.parent).is_some(), "{} parent missing", r.name);
            }
        }
    }

    #[test]
    fn factory_matches_parent() {
        for r in REGISTRATIONS.iter().filter(|r| r.parent == 0x0d) {
            assert_eq!(class(r.id).unwrap().factory, RENMASTER, "{}", r.name);
        }
    }

    #[test]
    fn ids_unique_and_fit_server_table() {
        let mut ids: Vec<u16> = REGISTRATIONS.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), REGISTRATIONS.len());
        assert!(ids.iter().all(|&i| (i as usize) < crate::routing::SERVER_TABLE_LEN));
    }
}
