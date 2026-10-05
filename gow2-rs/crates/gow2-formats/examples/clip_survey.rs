//! Which WAD holds which hero clips? `cargo run --example clip_survey -- <clip name>... -- <WAD>...`: for each WAD, the rig's animation record is searched for each name.

use gow2_formats::{skin, wad};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let sep = a.iter().position(|s| s == "--").unwrap_or(a.len());
    let (names, wads) = (&a[..sep], &a[(sep + 1).min(a.len())..]);
    for w in wads {
        let Ok(data) = std::fs::read(w) else { continue };
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let first: std::collections::HashMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
        for rr in skin::find_rigs(&recs) {
            let Some(anm) = rr.anm.as_deref().and_then(|n| first.get(n)) else { continue };
            let nj = gow2_formats::le_u32_pub(rr.rig, 4);
            let found: Vec<&String> = names.iter().filter(|n| skin::clip_channels(anm, nj, Some(n.as_str())).is_some()).collect();
            println!("{w}: model {} anm {:?} ({} bytes), {} joints, found {}/{}: {:?}", rr.model, rr.anm, anm.len(), nj, found.len(), names.len(), found);
        }
    }
}
