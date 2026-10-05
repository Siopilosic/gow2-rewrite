//! Prints the GIF-tag-like header entries of a model's batches (the V4-32 unpack): loop count, PRIM field, the other words, and how the ADC flags of the positions line up with the tag boundaries.
//! `cargo run --release -p gow2-formats --example mdl_tags -- ../extracted/pak/R_HERO01.WAD hero [max batches]`
use std::collections::BTreeMap;

use gow2_formats::{mdl, wad};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let model = a.next().expect("model name");
    let show: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let name = wad::mesh_record_name(&model);
    let blob = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("model blob").data;
    let mut prims: BTreeMap<i64, usize> = BTreeMap::new();
    let mut shown = 0;
    let mut mismatched = 0;
    let mut total_tags = 0;
    for (pi, part) in mdl::parts(blob).iter().enumerate() {
        for &(_, s, e) in &part.packets {
            for b in mdl::batches(blob, s, e) {
                let (Some(p), Some(h)) = (&b.pos, &b.hdr) else { continue };
                let mut start = 0usize;
                let mut line = String::new();
                for t in h {
                    let nl = (t[0] & 0x7FFF) as usize;
                    let prim = (t[1] >> 15) & 0x7FF;
                    *prims.entry(prim).or_default() += 1;
                    total_tags += 1;
                    // ADC flags of the first vertices of this tag's range
                    let adc = |i: usize| p.get(i).map_or('?', |v| if v[3] & 0x8000 != 0 { '1' } else { '0' });
                    let pattern: String = (start..(start + nl).min(p.len())).map(adc).collect();
                    // a strip starts with two ADC-set vertices in the PS2 convention: check
                    if !(pattern.starts_with("11") || pattern.starts_with("1")) {
                        mismatched += 1;
                    }
                    if shown < show {
                        line += &format!("  tag nl {nl} eop {} prim {prim:#x} words {:08x} {:08x} {:08x} adc {}\n", (t[0] >> 15) & 1, t[1], t[2], t[3], pattern);
                    }
                    start += nl;
                }
                if shown < show {
                    println!("part {pi} batch verts {} tags {}:\n{line}", p.len(), h.len());
                    shown += 1;
                }
            }
        }
    }
    println!("prim histogram {prims:?}; tags {total_tags}; tags not starting with an ADC-set vertex {mismatched}");
}
