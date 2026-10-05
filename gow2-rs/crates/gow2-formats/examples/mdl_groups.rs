//! Lists the parts of a model blob: kind, header words, DMA groups and the vertices and triangles each group decodes to.
//! `cargo run --release -p gow2-formats --example mdl_groups -- ../extracted/pak/R_HERO01.WAD hero`
use gow2_formats::{mdl, wad};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let model = a.next().expect("model name");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let name = wad::mesh_record_name(&model);
    let blob = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("model blob").data;
    let u32_at = |o: usize| u32::from_le_bytes(blob[o..o + 4].try_into().unwrap());
    for (n, p) in mdl::parts(blob).iter().enumerate() {
        let c = p.offset;
        let words: Vec<String> = (0..8).map(|i| format!("{:08x}", u32_at(c + 4 * i))).collect();
        let ngroups = blob[c + 0x18] as usize * u32_at(c + 0xC) as usize;
        let mut per_group: Vec<(usize, usize, usize)> = Vec::new(); // (group, packets, vertices)
        for g in 0..ngroups.max(1) {
            let mut verts = 0;
            for &(pg, s, e) in &p.packets {
                if pg == g {
                    verts += mdl::batches(blob, s, e).iter().filter_map(|b| b.pos.as_ref()).map(|v| v.len()).sum::<usize>();
                }
            }
            per_group.push((g, p.packets.iter().filter(|x| x.0 == g).count(), verts));
        }
        println!("part {n:2} kind {:#06x} words {} groups {} packets {:?}", p.kind, words.join(" "), ngroups, per_group);
    }
}
