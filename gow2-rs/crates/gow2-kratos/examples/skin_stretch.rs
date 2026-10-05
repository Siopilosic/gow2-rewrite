//! Finds where the skinning tears the model: for clips of the hero's bank, every triangle edge is measured in the bind pose and in the clip pose; edges that grow or shrink a lot
//! are counted per pair of joints (the base joints of their two vertices). `cargo run --release --example skin_stretch -- <hero WAD> [clip,clip,...]`
use std::collections::BTreeMap;

use gow2_formats::{skin, wad};
use gow2_skel::{local_parts, skin_matrices, skin_positions_blend, world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first[rr.anm.as_deref().unwrap()];
    let sm = skin::mesh_joints(first["MDL_hero_0"], 4096.0);
    let part_local = local_parts(&sm, &skel);
    let local: Vec<bool> = sm.part.iter().map(|&p| part_local[p as usize]).collect();
    let bind_pos: Vec<[f32; 3]> = sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]).collect();
    let bind_world = world_matrices(&skel, &skel.bind);
    let bind_skin = skin_matrices(&skel, &bind_world);
    let bind_posed = skin_positions_blend(&bind_pos, &sm.joints, &sm.joints2, &sm.weight, &local, &bind_skin, &bind_world);
    let names = a.get(2).map(|s| s.split(',').map(|x| x.to_string()).collect::<Vec<_>>()).unwrap_or_else(|| ["navIdle", "navCombatIdle", "attLunge", "navWalkSlow"].iter().map(|s| s.to_string()).collect());
    let jn = |j: i64| skel.names.get(j as usize).cloned().unwrap_or_default();
    for name in names {
        let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(&name)) else {
            println!("{name}: not decodable");
            continue;
        };
        let clip = Clip::bake(&cc, &skel);
        let mut tally: BTreeMap<(String, String), (usize, f32, f32)> = BTreeMap::new();
        let mut total = 0usize;
        for k in 0..6 {
            let t = clip.duration * k as f32 / 6.0;
            let pose = clip.sample(&skel, t);
            let world = world_matrices(&skel, &pose);
            let sk = skin_matrices(&skel, &world);
            if std::env::var_os("STRETCH_MATS").is_some() && k == 0 {
                for jname in ["pelvis", "vertebrae1", "vertebrae4", "neck", "head", "lFemur", "rFemur"] {
                    if let Some(j) = skel.names.iter().position(|n| n == jname) {
                        let m = &sk[j];
                        let len = |o: usize| (m[o] * m[o] + m[o + 1] * m[o + 1] + m[o + 2] * m[o + 2]).sqrt();
                        println!("  {jname:11} skin matrix column lengths {:.3} {:.3} {:.3} translation {:.1} {:.1} {:.1}; pose scale {:?}", len(0), len(4), len(8), m[12], m[13], m[14], pose[j].s);
                    }
                }
            }
            let out = skin_positions_blend(&bind_pos, &sm.joints, &sm.joints2, &sm.weight, &local, &sk, &world);
            for (tri, _) in &sm.tris {
                for e in 0..3 {
                    let (x, y) = (tri[e] as usize, tri[(e + 1) % 3] as usize);
                    let d = |p: &[[f32; 3]]| ((p[x][0] - p[y][0]).powi(2) + (p[x][1] - p[y][1]).powi(2) + (p[x][2] - p[y][2]).powi(2)).sqrt();
                    let (b, o) = (d(&bind_posed), d(&out));
                    if b < 0.3 {
                        continue;
                    }
                    total += 1;
                    let ratio = o / b;
                    if std::env::var_os("STRETCH_WORST").is_some() && (ratio < 0.2 || ratio > 4.0) && k == 0 {
                        let desc = |v: usize| format!("part {} {}->{} w {:.2} bind {:.1?} posed {:.1?}", sm.part[v], jn(sm.joints[v]), jn(sm.joints2[v]), sm.weight[v], bind_posed[v], out[v]);
                        println!("      ratio {ratio:.2}: A {} | B {}", desc(x), desc(y));
                        for v in [x, y] {
                            let (ja, jb) = (sm.joints[v] as usize, sm.joints2[v] as usize);
                            let pa = gow2_skel::transform_point(&sk[ja], bind_pos[v]);
                            let pb = gow2_skel::transform_point(&sk[jb], bind_pos[v]);
                            println!("          vertex {v} local {} bind_pos {:.2?} via base {pa:.2?} via second {pb:.2?} weight {:.3}", local[v], bind_pos[v], sm.weight[v]);
                        }
                    }
                    if ratio > 1.8 || ratio < 0.35 {
                        let key = (jn(sm.joints[x]), jn(sm.joints[y]));
                        let e = tally.entry(key).or_insert((0, 0.0, 0.0));
                        e.0 += 1;
                        e.1 = e.1.max(ratio);
                        e.2 = if e.2 == 0.0 { ratio } else { e.2.min(ratio) };
                    }
                }
            }
        }
        let bad: usize = tally.values().map(|v| v.0).sum();
        println!("{name}: {bad} of {total} edge samples stretched more than 1.8x or shrunk below 0.35x");
        let mut v: Vec<_> = tally.into_iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(c.0));
        for ((ja, jb), (n, hi, lo)) in v.into_iter().take(12) {
            println!("    {ja:14} - {jb:14} {n:5} edges, ratio {lo:.2}..{hi:.2}");
        }
    }
}
