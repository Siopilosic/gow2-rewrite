//! Lists the rig of every model of a level WAD: joint count, animation, and the root joint's translation (row 3 of its matrix).
//! `cargo run --release -p gow2-formats --example rig_probe -- ../extracted/pak/RHOD20.WAD`
use gow2_formats::{level, skin, wad};

fn main() {
    let path = std::env::args().nth(1).expect("level WAD path");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let pl = level::parse(&recs);
    for rr in skin::find_rigs(&recs) {
        let rig = skin::parse_rig(rr.rig);
        let t = rig.mats.first().map(|m| [m[12], m[13], m[14]]).unwrap_or([0.0; 3]);
        let xf = pl.xf(&rr.model);
        println!(
            "{:24} joints {:3} anm {:5} in rigged set {:5} root t {:.0?} scale {:.3} record offset {:.0?}",
            rr.model,
            rig.mats.len(),
            rr.anm.is_some(),
            pl.rigged.contains(&rr.model),
            t,
            rig.mats.first().map(|m| m[0]).unwrap_or(0.0),
            xf.offset
        );
        if std::env::args().nth(2).is_some_and(|m| m == rr.model) {
            for (j, n) in rig.names.iter().enumerate() {
                println!("    joint {j:3} {n:24} parent {:3} t {:.1?}", rig.parents[j], [rig.mats[j][12], rig.mats[j][13], rig.mats[j][14]]);
            }
        }
    }
}
