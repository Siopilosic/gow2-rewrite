//! Where do the hand-held object's slot joints sit in a clip? Prints the hand and free joint positions (rig space) and the vertical direction of the arm, to see which
//! member of an aim group (`...00`, `...01`, `...02`) aims where. `cargo run --example aim_probe -- <hero WAD> <clip>[,<clip>...] [t0,t1,...]`

use gow2_formats::{skin, wad};
use gow2_skel::{world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first[rr.anm.as_deref().unwrap()];
    let find = |n: &str| skel.names.iter().position(|x| x.eq_ignore_ascii_case(n));
    let joints = ["rWeapIH", "rWeapOH", "lWeapIH", "rWrist", "lWrist", "pelvis", "head", "rHumerus", "lHumerus"];
    let idx: Vec<Option<usize>> = joints.iter().map(|n| find(n)).collect();
    println!("joints: {:?}", joints.iter().zip(&idx).collect::<Vec<_>>());
    let times: Vec<f32> = a.get(3).map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect()).unwrap_or_else(|| vec![0.0, 0.5]);
    for name in a[2].split(',') {
        let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(name)) else {
            println!("{name}: not decodable");
            continue;
        };
        if std::env::var_os("AIM_JOINTS").is_some() {
            let names: Vec<String> = cc.joints.iter().map(|(j, c)| format!("{}[{}{}{}]", skel.names[*j as usize], c[0].len(), c[1].len(), c[2].len())).collect();
            println!("{name}: {} joints animated: {}", cc.joints.len(), names.join(" "));
        }
        let clip = Clip::bake(&cc, &skel);
        for &f in &times {
            let t = f * clip.duration;
            let mut pose = clip.sample(&skel, t);
            // AIM_BASE=<clip>: lay this partial clip over a full-body base clip (joints the clip does not animate come from the base)
            if let Ok(bn) = std::env::var("AIM_BASE") {
                let bcc = skin::clip_channels(anm, skel.len() as u32, Some(&bn)).expect("base clip");
                let base = Clip::bake(&bcc, &skel);
                let bp = base.sample(&skel, (f * clip.duration) % base.duration.max(0.1));
                for j in 0..pose.len() {
                    let covered = clip.joints[j].rot.is_some() || clip.joints[j].trans.is_some() || clip.joints[j].scale.is_some();
                    if !covered {
                        pose[j] = bp[j];
                    }
                }
            }
            let w = world_matrices(&skel, &pose);
            let p = |j: Option<usize>| j.map_or([f32::NAN; 3], |j| [w[j][12], w[j][13], w[j][14]]);
            let pel = p(idx[5]);
            let rel = |j: Option<usize>| {
                let q = p(j);
                [q[0] - pel[0], q[1] - pel[1], q[2] - pel[2]]
            };
            // torso forward from the shoulder line (rig space: x right, -z forward): the heading angle of the forward vector, 0 = straight ahead, positive = turned to the right
            let (rs, ls) = (p(idx[7]), p(idx[8]));
            let (sx, sz) = (rs[0] - ls[0], rs[2] - ls[2]);
            let (fx, fz) = (sz, -sx);
            let yaw = (fx).atan2(-fz).to_degrees();
            if std::env::var_os("AIM_AXES").is_some() {
                for (label, j) in [("rWeapIH", idx[0]), ("rWeapOH", idx[1])] {
                    if let Some(j) = j {
                        let m = &w[j];
                        let n = |a: usize| { let l = (m[a] * m[a] + m[a + 1] * m[a + 1] + m[a + 2] * m[a + 2]).sqrt().max(1e-9); [m[a] / l, m[a + 1] / l, m[a + 2] / l] };
                        println!("    {label} lengths {:.2} {:.2} {:.2}", (m[0]*m[0]+m[1]*m[1]+m[2]*m[2]).sqrt(), (m[4]*m[4]+m[5]*m[5]+m[6]*m[6]).sqrt(), (m[8]*m[8]+m[9]*m[9]+m[10]*m[10]).sqrt());
                        println!("    {label} axes x {:.2?} y {:.2?} z {:.2?}", n(0), n(4), n(8));
                    }
                }
                let (a, b) = (p(idx[3]), p(idx[0]));
                println!("    wrist to hand joint {:.2?}", [b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            }
            println!("{name:22} torso yaw {yaw:6.1} dur {:.2} t {t:.2}: rWeapIH {:.1?} rWeapOH {:.1?} rWrist {:.1?} lWrist {:.1?}", clip.duration, rel(idx[0]), rel(idx[1]), rel(idx[3]), rel(idx[4]));
        }
    }
}





