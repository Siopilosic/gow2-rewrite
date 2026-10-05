//! The Rust texture resolver must reproduce `tools/gfx_decode.py` TextureStore exactly.
//! Expected values come from `tools/oracle_tex.py`: per MAT_ record the size and an FNV hash of the RGBA
//! pixels (width 0 = the material does not decode). No game data is stored.

use std::{collections::BTreeMap, path::PathBuf};

use gow2_formats::{texture::TextureStore, wad};

#[test]
fn rust_textures_match_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected = std::fs::read_to_string(root.join("gow2-rs/crates/gow2-formats/tests/oracle/tex.tsv")).unwrap();
    let mut by_wad: BTreeMap<&str, Vec<Vec<&str>>> = BTreeMap::new();
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        by_wad.entry(f[0]).or_default().push(f);
    }
    let mut checked = 0;
    for (w, rows) in by_wad {
        let p = root.join("extracted/pak").join(format!("{w}.WAD"));
        let Ok(data) = std::fs::read(&p) else {
            eprintln!("{} not found; skipping", p.display());
            continue;
        };
        let store = TextureStore::new(
            wad::records(&data).filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)),
        );
        for f in rows {
            let mat = f[1];
            let (ew, eh) = (f[2].parse::<u32>().unwrap(), f[3].parse::<u32>().unwrap());
            match store.material_texture(mat) {
                None => assert_eq!(ew, 0, "{w} {mat}: Rust failed to decode"),
                Some(t) => {
                    assert_eq!((t.width, t.height), (ew, eh), "{w} {mat} size");
                    let mut h: u64 = 1469598103934665603;
                    for &x in &t.rgba {
                        h = (h ^ x as u64).wrapping_mul(1099511628211);
                    }
                    assert_eq!(h.to_string(), f[4], "{w} {mat} pixels");
                }
            }
            checked += 1;
        }
    }
    eprintln!("{checked} materials matched the Python oracle");
}
