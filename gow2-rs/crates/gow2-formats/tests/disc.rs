//! Tests against the user's own disc image. They are skipped (pass trivially, with a
//! note) when the image is absent. Set GOW2_ISO to override the default path.
//! The full-disc WAD sweep reads ~630 MB and only runs with GOW2_FULL=1.

use std::path::PathBuf;

use gow2_formats::{iso::Iso, records, toc, wad};

fn iso_path() -> Option<PathBuf> {
    let p = std::env::var_os("GOW2_ISO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../God of War II.iso")));
    if p.exists() {
        Some(p)
    } else {
        eprintln!("disc image not found at {}; skipping", p.display());
        None
    }
}

fn open() -> Option<(Vec<toc::TocEntry>, toc::Pak)> {
    let mut iso = Iso::open(iso_path()?).unwrap();
    let (v, e) = iso.find("GODOFWAR.TOC").unwrap();
    let entries = toc::parse_toc(&iso.read_file(v, &e).unwrap());
    Some((entries, toc::Pak::open(iso).unwrap()))
}

#[test]
fn toc_has_913_files_and_two_layers() {
    let Some((entries, pak)) = open() else { return };
    assert_eq!(entries.len(), 913);
    assert_eq!(pak.iso.volumes.len(), 2);
    assert!(entries.iter().any(|e| e.copies.iter().any(|&s| s >= toc::PART2_BASE)));
}

#[test]
fn duplicate_copies_are_identical() {
    let Some((entries, mut pak)) = open() else { return };
    for e in entries.iter().filter(|e| e.copies.len() > 1).take(40) {
        let first = pak.read(e.copies[0], e.size.min(4096)).unwrap();
        for &c in &e.copies[1..] {
            assert_eq!(pak.read(c, e.size.min(4096)).unwrap(), first, "{}", e.name);
        }
    }
}

fn check_wad(name: &str, data: &[u8]) {
    let mut end = 0usize;
    for r in wad::records(data) {
        end = r.offset + ((0x20 + r.data.len() + 15) & !15);
        if r.tag == wad::Tag::Object && r.data.len() >= 4 {
            let t = u32::from_le_bytes(r.data[..4].try_into().unwrap());
            if t == 7 {
                assert!(records::parse_txr(r.data).is_some(), "{name}/{}: TXR payload not 88 bytes", r.name);
            }
            if t & 0x8000_0000 != 0 {
                let ip = records::parse_instance(r.data).expect("instance payload");
                assert_eq!(ip.field_04, 0, "{name}/{}", r.name);
            }
        }
    }
    assert_eq!(end, data.len(), "{name} does not parse to EOF");
}

#[test]
fn perma_wad_parses_and_layouts_hold() {
    let Some((entries, mut pak)) = open() else { return };
    let e = entries.iter().find(|e| e.name == "R_PERMA.WAD").unwrap();
    check_wad(&e.name, &pak.read_entry(e).unwrap());
}

#[test]
fn every_wad_parses_and_layouts_hold() {
    if std::env::var_os("GOW2_FULL").is_none() {
        eprintln!("set GOW2_FULL=1 for the full-disc sweep");
        return;
    }
    let Some((entries, mut pak)) = open() else { return };
    let mut n = 0;
    for e in entries.iter().filter(|e| e.name.ends_with(".WAD") && e.name != "R_MCICON.WAD") {
        check_wad(&e.name, &pak.read_entry(e).unwrap());
        n += 1;
    }
    assert_eq!(n, 292);
}
