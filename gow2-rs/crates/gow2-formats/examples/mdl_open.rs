//! Compares triangle rules for the skinned model parts by counting open (boundary) edges after welding vertices that share a position, and orphan vertices.
//! `cargo run --release -p gow2-formats --example mdl_open -- ../extracted/pak/R_HERO01.WAD hero`
use std::collections::HashMap;

use gow2_formats::{mdl, wad};

/// (part index, [vertex positions of a strip], [adc flags]) for every GIF-tag strip of the model.
type Strip = (usize, Vec<[i64; 3]>, Vec<bool>);

fn strips(blob: &[u8]) -> Vec<Strip> {
    let mut out = Vec::new();
    for (pi, part) in mdl::parts(blob).iter().enumerate() {
        for &(g, s, e) in &part.packets {
            if g != 0 {
                continue;
            }
            for b in mdl::batches(blob, s, e) {
                let Some(p) = &b.pos else { continue };
                out.push((pi, p.iter().map(|v| [v[0], v[1], v[2]]).collect(), p.iter().map(|v| v[3] & 0x8000 != 0).collect()));
            }
        }
    }
    out
}

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let model = a.next().expect("model name");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let name = wad::mesh_record_name(&model);
    let blob = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("model blob").data;
    let all = strips(blob);
    let nparts = all.iter().map(|s| s.0).max().map_or(0, |m| m + 1);
    // rule 0: triangle n drawn when its vertex has no ADC flag (current). rule 1: drawn for every n >= 2 (flags ignored).
    // rule 2: drawn when no ADC flag on n, and on the vertex after a flagged pair the strip restarts (n-2, n-1 not both flagged).
    for rule in 0..3 {
        let (mut total_open, mut total_tris, mut total_orphans) = (0, 0, 0);
        let mut per_part = Vec::new();
        for part in 0..nparts {
            let mut weld: HashMap<[i64; 3], u32> = HashMap::new();
            let mut used: HashMap<u32, bool> = HashMap::new();
            let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
            let mut tris = 0;
            for (_, v, adc) in all.iter().filter(|s| s.0 == part) {
                let ids: Vec<u32> = v
                    .iter()
                    .map(|p| {
                        let n = weld.len() as u32;
                        *weld.entry(*p).or_insert(n)
                    })
                    .collect();
                for &i in &ids {
                    used.entry(i).or_insert(false);
                }
                for n in 2..v.len() {
                    let draw = match rule {
                        0 => !adc[n],
                        1 => true,
                        _ => !adc[n] || (!adc[n - 1] && !adc[n - 2]),
                    };
                    let w = [ids[n - 2], ids[n - 1], ids[n]];
                    if !draw || w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
                        continue;
                    }
                    tris += 1;
                    for &i in &w {
                        used.insert(i, true);
                    }
                    for k in 0..3 {
                        let (x, y) = (w[k], w[(k + 1) % 3]);
                        *edges.entry((x.min(y), x.max(y))).or_insert(0) += 1;
                    }
                }
            }
            let open = edges.values().filter(|&&c| c == 1).count();
            let orphans = used.values().filter(|&&u| !u).count();
            total_open += open;
            total_tris += tris;
            total_orphans += orphans;
            per_part.push(format!("{part}:{open}/{orphans}"));
        }
        println!("rule {rule}: tris {total_tris} open edges {total_open} orphan verts {total_orphans}   per part open/orphans {}", per_part.join(" "));
    }
}
