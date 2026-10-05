//! The HUD movie parses with the loader's layout and its shapes decode (`docs/hud.md`).

use std::path::PathBuf;

use gow2_formats::{flp, wad};

fn perma() -> Option<Vec<u8>> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD")).ok()
}

#[test]
fn the_hud_movie_parses_to_the_end_of_its_record() {
    let Some(data) = perma() else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let m = recs.iter().find(|r| r.name == "FLP_HUDA" && !r.data.is_empty()).unwrap();
    let f = flp::Flp::parse(m.data).expect("layout walk ends at the record end");
    assert_eq!(f.chars.len(), 1404);
    assert_eq!(f.shapes.len(), 277);
    assert_eq!(f.clips.len(), 477);
    assert_eq!(f.root.frames, 87);
    assert_eq!(f.root.layers.len(), 43);
    assert_eq!(f.root.label_frame("HUD"), Some(1));
    // the main meter group is placed on the root, and the bar clips are named
    let names: Vec<&str> = f.root.layers.iter().flatten().filter_map(|k| f.string(k.name)).collect();
    assert!(names.contains(&"MainMeterT"), "{names:?}");
    // the meter well has one label per bar level
    let well = &f.clips[29];
    assert_eq!(well.label_frame("BarLevel0"), Some(51));
    assert_eq!(well.label_frame("BarLevel4"), Some(252));
}

#[test]
fn the_shape_model_decodes_quads_and_meshes() {
    let Some(data) = perma() else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let mdl = recs.iter().find(|r| r.name == "MDL_HUDA_0" && !r.data.is_empty()).unwrap();
    let shapes = flp::parse_shapes(mdl.data);
    assert_eq!(shapes.len(), 410);
    // shape 146 is the 256 x 96 px frame: one textured quad, x from -2560 to 2560 twips
    let frame = &shapes[146];
    assert_eq!(frame.items.len(), 1);
    let v = &frame.items[0].verts;
    assert_eq!(v.len(), 4);
    assert!(frame.items[0].textured);
    assert_eq!(v.iter().map(|p| p.x as i32).max(), Some(2560));
    assert_eq!(v.iter().map(|p| p.x as i32).min(), Some(-2560));
    assert_eq!(frame.items[0].triangles().len(), 2);
    // the texture group lists the frame's bitmap where the movie indexes it
    let tags: Vec<(wad::Tag, String)> = recs.iter().map(|r| (r.tag, r.name.clone())).collect();
    let group = flp::texture_group(&tags, "FLP_HUDA");
    assert_eq!(group.len(), 91);
    assert_eq!(group[12], "TXR_HUDA010");
}
