//! Lists the clips of every rigged model's animation record in a WAD: name and duration.
//! `cargo run --release -p gow2-formats --example clip_list -- ../extracted/pak/R_RHSOLD00.WAD`
use gow2_formats::{anm, skin, wad};

fn main() {
    let path = std::env::args().nth(1).expect("WAD path");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::HashMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    for rr in skin::find_rigs(&recs) {
        let Some(a) = rr.anm.as_deref().and_then(|n| first.get(n)) else { continue };
        let nj = gow2_formats::le_u32_pub(rr.rig, 4);
        let list = anm::clips(a);
        println!("model {} anm {:?}: {} joints, {} clips", rr.model, rr.anm, nj, list.len());
        for c in list {
            println!("  {:<40} {:.3} s", skin::clip_name(a, c), anm::clip_duration(a, c).unwrap_or(0.0));
        }
    }
}
