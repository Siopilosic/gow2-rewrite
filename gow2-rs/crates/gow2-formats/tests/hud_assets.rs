//! The HUD artwork in `R_PERMA.WAD` decodes through `TXR_HUDA*` without materials (`docs/hud.md`).

use std::path::PathBuf;

use gow2_formats::{texture::TextureStore, wad};

#[test]
fn the_hud_sprites_decode_with_their_known_sizes() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD");
    let Ok(data) = std::fs::read(p) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    // the blade-shaped frame, the red orb, the gold orb and the dark emblem
    for (name, w, h) in [("TXR_HUDA010", 256, 96), ("TXR_HUDA_G011", 64, 64), ("TXR_HUDA011", 32, 32), ("TXR_HUDA009", 64, 64)] {
        let t = store.txr_texture(name).unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!((t.width, t.height), (w, h), "{name}");
        assert_eq!(t.rgba.len(), (w * h * 4) as usize);
        assert!(t.rgba.chunks(4).any(|p| p[3] > 200), "{name} has opaque pixels");
    }
    // the frame is mostly the grey well and the metal: it is opaque over a good part of its area
    let f = store.txr_texture("TXR_HUDA010").unwrap();
    let opaque = f.rgba.chunks(4).filter(|p| p[3] > 200).count();
    assert!(opaque > 256 * 96 / 3, "{opaque} opaque pixels");
    // the movie itself is a record of the same WAD
    assert!(recs.iter().any(|r| r.name == "FLP_HUDA" && r.data.len() > 400_000));
}
