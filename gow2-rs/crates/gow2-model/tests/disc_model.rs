//! Checks the recovered model against disc data (skipped when the image is absent).

use std::path::PathBuf;

use gow2_formats::{iso::Iso, toc};
use gow2_model::routing;

fn wad(name: &str) -> Option<Vec<u8>> {
    let p = std::env::var_os("GOW2_ISO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../God of War II.iso")));
    if !p.exists() {
        eprintln!("disc image not found; skipping");
        return None;
    }
    let mut iso = Iso::open(p).unwrap();
    let (v, e) = iso.find("GODOFWAR.TOC").unwrap();
    let entries = toc::parse_toc(&iso.read_file(v, &e).unwrap());
    let mut pak = toc::Pak::open(iso).unwrap();
    let e = entries.iter().find(|e| e.name == name)?;
    Some(pak.read_entry(e).unwrap())
}

#[test]
fn group_semantics_and_routing_hold_for_shell_and_perm_wads() {
    for name in ["R_PERMA.WAD", "R_SHELLA.WAD"] {
        let Some(data) = wad(name) else { return };
        let rep = routing::route(&data);
        assert!(rep.unknown_servers.is_empty(), "{name}: {:?}", rep.unknown_servers);
        assert_eq!(rep.group_start_while_pending, 0, "{name}");
        assert_eq!(rep.group_start_not_followed_by_object, 0, "{name}");
        assert!(rep.max_group_depth <= routing::GROUP_STACK_DEPTH, "{name}");
        assert!(rep.instance_records > 0, "{name}");
    }
}
