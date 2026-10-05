use gow2_formats::{skin, wad};
use gow2_skel::Skeleton;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    for (j, n) in skel.names.iter().enumerate() {
        if ["Clavicle", "Humerus", "Radius", "Wrist"].iter().any(|k| n.contains(k)) && !n.contains("Thumb") {
            let p = skel.parents[j];
            println!("{j:3} {n:12} parent {:3} {:12} local t {:.1?} bind world pos {:.1?}", p, if p >= 0 { skel.names[p as usize].clone() } else { String::new() }, skel.bind[j].t, [skel.bind_world[j][12], skel.bind_world[j][13], skel.bind_world[j][14]]);
        }
    }
}
