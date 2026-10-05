//! The Rust MDL decoder must reproduce the Python decoder (`tools/mdl_decode.py`) exactly.
//! Expected values come from `tools/oracle_mdl.py` (counts and a 64-bit FNV checksum, no game data).
//! Reads `extracted/pak/<WAD>.WAD` next to the workspace; a missing WAD is skipped with a note.

use std::path::PathBuf;

use gow2_formats::{mdl, wad};

fn fnv(h: &mut u64, x: i64) {
    *h = (*h ^ (x as u64)) .wrapping_mul(1099511628211);
}

#[test]
fn rust_mdl_matches_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected = std::fs::read_to_string(root.join("gow2-rs/crates/gow2-formats/tests/oracle/mdl.tsv")).unwrap();
    let mut checked = 0;
    let mut cache: Option<(String, Vec<u8>)> = None;
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let (w, name, group) = (f[0], f[1], f[2].parse::<usize>().unwrap());
        let (verts, tris, check) = (f[3].parse::<usize>().unwrap(), f[4].parse::<usize>().unwrap(), f[5]);
        if cache.as_ref().map(|c| c.0.as_str()) != Some(w) {
            let p = root.join("extracted/pak").join(format!("{w}.WAD"));
            let Ok(data) = std::fs::read(&p) else {
                eprintln!("{} not found; skipping", p.display());
                continue;
            };
            cache = Some((w.to_string(), data));
        }
        let data = &cache.as_ref().unwrap().1;
        let rec = wad::records(data)
            .find(|r| r.tag == wad::Tag::Object && r.name == name && r.data.len() > 0x100)
            .expect("record present");
        let m = mdl::mesh(rec.data, group);
        let mut h: u64 = 1469598103934665603;
        for v in &m.verts {
            v.iter().for_each(|&x| fnv(&mut h, x));
        }
        for c in &m.cols {
            c.iter().for_each(|&x| fnv(&mut h, x));
        }
        for t in &m.tris {
            t.iter().for_each(|&x| fnv(&mut h, x as i64));
        }
        assert_eq!((m.verts.len(), m.tris.len()), (verts, tris), "{w} {name} group {group}");
        assert_eq!(h.to_string(), check, "{w} {name} group {group} checksum");
        checked += 1;
    }
    eprintln!("{checked} model parts matched the Python oracle");
}
