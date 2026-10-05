//! The Rust skin module must reproduce `tools/rig_anim.py`: rig tree, joint-bound meshes, clip channels.
//! Expected values come from `tools/oracle_skin.py` (counts and 64-bit FNV hashes, no game data).

use std::{collections::BTreeMap, path::PathBuf};

use gow2_formats::{anm, skin, wad};

struct H(u64);

impl H {
    fn new() -> Self {
        H(1469598103934665603)
    }
    fn i(&mut self, x: i64) {
        self.0 = (self.0 ^ x as u64).wrapping_mul(1099511628211);
    }
    fn f32(&mut self, x: f32) {
        self.i(x.to_bits() as i64);
    }
    fn f64(&mut self, x: f64) {
        self.i(x.to_bits() as i64);
    }
    fn s(&mut self, t: &str) {
        t.chars().for_each(|c| self.i(c as u32 as i64)); // names are latin-1 chars
        self.i(0xFF);
    }
}

fn hash_clip(res: Option<skin::ClipChannels>) -> String {
    let Some(r) = res else { return "none".into() };
    let mut h = H::new();
    h.f32(r.dt);
    h.f32(r.duration);
    for (j, kinds) in &r.joints {
        h.i(*j as i64);
        for (ki, comps) in kinds.iter().enumerate() {
            h.i(ki as i64);
            for (c, frames) in comps {
                h.i(*c as i64);
                for (f, v) in frames {
                    h.i(*f);
                    h.f64(*v);
                }
            }
        }
    }
    h.0.to_string()
}

#[test]
fn rust_skin_matches_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected = std::fs::read_to_string(root.join("gow2-rs/crates/gow2-formats/tests/oracle/skin.tsv")).unwrap();
    let mut by_wad: BTreeMap<&str, Vec<Vec<&str>>> = BTreeMap::new();
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        by_wad.entry(f[0]).or_default().push(f);
    }
    let mut checked = 0;
    for (w, rows) in by_wad {
        let p = root.join("extracted/pak").join(format!("{w}.WAD"));
        let Ok(data) = std::fs::read(&p) else {
            eprintln!("{} not found; skipping", p.display());
            continue;
        };
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let first: BTreeMap<&str, &[u8]> = recs
            .iter()
            .filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty())
            .fold(BTreeMap::new(), |mut m, r| {
                m.entry(r.name.as_str()).or_insert(r.data);
                m
            });
        let rigs = skin::find_rigs(&recs);
        for f in rows {
            let model = f[1];
            let rr = rigs.iter().find(|r| r.model == model).expect("rig present");
            let rig = skin::parse_rig(rr.rig);
            let nj = rig.parents.len();
            assert_eq!(nj.to_string(), f[2], "{w} {model} joint count");
            let mut h = H::new();
            rig.parents.iter().for_each(|&x| h.i(x as i64));
            for m in &rig.mats {
                m.iter().for_each(|&x| h.f32(x));
                let (t, q, s) = skin::mat_to_trs(m);
                t.iter().chain(q.iter()).chain(s.iter()).for_each(|&x| h.f64(x));
            }
            rig.names.iter().for_each(|n| h.s(n));
            assert_eq!(h.0.to_string(), f[3], "{w} {model} rig");

            let (mut vs, mut tris, mut mh) = (0, 0, "none".to_string());
            if let Some(blob) = first.get(format!("MDL_{model}_0").as_str()).filter(|b| b.len() > 64) {
                let m = skin::mesh_joints(blob, 4096.0);
                let mut h = H::new();
                m.verts.iter().for_each(|v| v.iter().for_each(|&x| h.i(x)));
                m.uvs.iter().for_each(|v| v.iter().for_each(|&x| h.f64(x)));
                m.cols.iter().for_each(|c| c.iter().for_each(|&x| h.i(x)));
                m.joints.iter().for_each(|&x| h.i(x));
                for (t, slot) in &m.tris {
                    t.iter().for_each(|&x| h.i(x as i64));
                    h.i(*slot as i64);
                }
                (vs, tris, mh) = (m.verts.len(), m.tris.len(), h.0.to_string());
            }
            assert_eq!((vs.to_string(), tris.to_string(), mh), (f[4].to_string(), f[5].to_string(), f[6].to_string()), "{w} {model} mesh");

            let (mut c0, mut named) = ("none".to_string(), "none".to_string());
            if let Some(a) = rr.anm.as_deref().and_then(|n| first.get(n)).filter(|a| a.len() > 0x40) {
                c0 = hash_clip(skin::clip_channels(a, nj as u32, None));
                let mut found: Vec<(String, String)> = Vec::new();
                for c in anm::clips(a) {
                    let nm = skin::clip_name(a, c);
                    if nm.is_empty() || found.iter().any(|x| x.0 == nm) {
                        continue;
                    }
                    if let Some(res) = skin::clip_channels(a, nj as u32, Some(&nm)) {
                        if !res.joints.is_empty() {
                            found.push((nm, hash_clip(Some(res))));
                        }
                    }
                    if found.len() == 3 {
                        break;
                    }
                }
                if !found.is_empty() {
                    named = found.iter().map(|(a, b)| format!("{a}:{b}")).collect::<Vec<_>>().join(",");
                }
            }
            assert_eq!(c0, f[7], "{w} {model} first clip");
            assert_eq!(named, f[8], "{w} {model} named clips");
            checked += 1;
        }
    }
    eprintln!("{checked} rigs matched the Python oracle");
}
