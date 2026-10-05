//! The effect data of a WAD: particle shapes (`PTC_`), emitters (`FXC_`), emission geometry (`NCV_`, `MSH_`) and the effect rigs that group them.
//!
//! Layouts are those of `docs/particles.md` (sections 3, 3.1, 3.4, 4, 4.2, 4.3); the Python decoders `tools/ptc_decode.py` and `tools/ptc_export.py` are the oracle.

use std::collections::{BTreeMap, HashMap};

use gow2_formats::{anm, wad};

fn u16at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn f32at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32at(b, o))
}
fn cstr(b: &[u8], o: usize, n: usize) -> String {
    let s = b.get(o..(o + n).min(b.len())).unwrap_or(&[]);
    let end = s.iter().position(|&x| x == 0).unwrap_or(s.len());
    s[..end].iter().map(|&c| c as char).collect()
}

/// How a shape's particles are blended into the frame (the GS ALPHA register, `docs/particles.md` 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// `(Cs - Cd) * As + Cd`
    Normal,
    /// `Cs * As + Cd`
    Additive,
    /// `Cd - Cs * As`
    Subtractive,
}

/// A particle shape: lifetime, flags and the program (eight lists and a data table) that the game's VU1 program A evaluates per particle.
#[derive(Debug, Clone)]
pub struct Shape {
    pub name: String,
    /// The name emitters refer to (record `+0x54`).
    pub shape_name: String,
    /// Seconds; zero or negative means the particle never expires.
    pub life: f32,
    pub flags: u32,
    /// Render routine index (`docs/particles.md` 3.3): 3 is the rotated billboard, 4 a screen disc, 1 a sprite, 7 a streak.
    pub render: u32,
    pub lists: [Vec<u16>; 8],
    pub data: Vec<[f32; 4]>,
    /// The `MAT_` record that follows the shape in its group.
    pub material: Option<String>,
}

impl Shape {
    pub fn blend(&self) -> Blend {
        let f = self.flags;
        let pick = |k: u32| match k {
            0 => Blend::Normal,
            1 => Blend::Additive,
            2 => Blend::Subtractive,
            _ => Blend::Normal,
        };
        if f & 0x6040 != 0 && f & 3 != 3 {
            pick(f & 3)
        } else if f & 0x4040 != 0 {
            Blend::Normal
        } else {
            // flags & 0x2000 keeps whatever blend was set before: taken as normal
            pick(f & 3)
        }
    }

    /// The shape writes depth (`0x20000`).
    pub fn zwrite(&self) -> bool {
        self.flags & 0x20000 != 0
    }

    fn parse(name: &str, b: &[u8], material: Option<String>) -> Shape {
        let dsize = u32at(b, 0xA0) as usize;
        let data: Vec<[f32; 4]> = (0..dsize / 16).map(|q| [f32at(b, 0x128 + 16 * q), f32at(b, 0x128 + 16 * q + 4), f32at(b, 0x128 + 16 * q + 8), f32at(b, 0x128 + 16 * q + 12)]).collect();
        let mut lists: [Vec<u16>; 8] = Default::default();
        for (g, list) in lists.iter_mut().enumerate() {
            for k in 0..8usize {
                // the eight u16 of a list are stored interleaved: slot k at short (k & 3) * 2 + (k >> 2)
                let v = u16at(b, 0xA8 + 16 * g + 2 * (((k & 3) << 1) | (k >> 2))) as i16;
                if v == -1 {
                    break;
                }
                list.push(v as u16);
            }
        }
        Shape { name: name.to_string(), shape_name: cstr(b, 0x54, 24), life: f32at(b, 0x70), flags: u32at(b, 0x80), render: u32at(b, 0x9C), lists, data, material }
    }
}

/// An emitter record (`FXC_`, subtypes other than 11 trail, 12 field, 13 geometry).
#[derive(Debug, Clone)]
pub struct Emitter {
    pub name: String,
    pub subtype: u16,
    pub shape_name: String,
    /// Row-vector local matrix (record `+0x10`).
    pub matrix: [f32; 16],
    /// Parent joint of the effect's skeleton (record `+0x08`), -1 for none.
    pub joint: i16,
    /// Animation block that drives the emitter's rate (record `+0x0a`), 0xffff for none.
    pub channel: u16,
    /// The parameter block `P`, record `+0x84` (`docs/particles.md` 4).
    pub p: [f32; 24],
    /// Name of the emission geometry (subtypes 3 and 4, record `+0x6c`).
    pub geom: Option<String>,
}

impl Emitter {
    pub fn spread(&self) -> f32 {
        self.p[3]
    }
    pub fn speed(&self) -> f32 {
        self.p[4]
    }
    pub fn speed_range(&self) -> f32 {
        self.p[5]
    }
    pub fn radius(&self) -> [f32; 2] {
        [self.p[6], self.p[7]]
    }
    /// The rate the record carries (particles per second); a rate track overrides it.
    pub fn rate(&self) -> f32 {
        self.p[8]
    }
}

/// Emission geometry: a cubic curve (`NCV_`) or a mesh (`MSH_`), with the geometry record's matrix.
#[derive(Debug, Clone)]
pub enum GeomKind {
    Curve { segs: Vec<f32>, knots: Vec<f32> },
    Mesh { verts: Vec<f32>, tris: Vec<u16> },
}

#[derive(Debug, Clone)]
pub struct Geometry {
    pub matrix: [f32; 16],
    pub kind: GeomKind,
}

/// The rate of one emitter over the clip: keys `(frame, rate)` at `dt` seconds per frame (`docs/particles.md` 4, type-10 animation track, slot 8).
#[derive(Debug, Clone, Default)]
pub struct RateTrack {
    pub dt: f32,
    pub duration: f32,
    pub keys: Vec<(i64, f32)>,
}

impl RateTrack {
    /// The rate `t` seconds into the clip (linear between keys, zero outside them).
    pub fn at(&self, t: f32) -> f32 {
        let f = t / self.dt.max(1e-6);
        let k = &self.keys;
        if k.is_empty() || f < k[0].0 as f32 || f > k[k.len() - 1].0 as f32 + 1.0 {
            return 0.0;
        }
        for w in k.windows(2) {
            let (a, b) = (w[0], w[1]);
            if f >= a.0 as f32 && f <= b.0 as f32 {
                let u = (f - a.0 as f32) / ((b.0 - a.0) as f32).max(1e-6);
                return a.1 + (b.1 - a.1) * u;
            }
        }
        k[k.len() - 1].1
    }
}

/// A group of emitters that belong together: the effect the game spawns by name (an unnamed rig record followed by `MDL_`, `ANM_` and `FXC_` references).
#[derive(Debug, Clone)]
pub struct EffectDef {
    /// The `go` node whose group holds the rig (its name, lower case as stored), if any.
    pub go: Option<String>,
    pub model: Option<String>,
    pub anm: Option<String>,
    /// The rig record's bytes (`gow2_formats::skin::parse_rig`), for the joints the emitters hang under.
    pub rig: Vec<u8>,
    pub emitters: Vec<String>,
    /// ANM block index -> rate track (from the `ANM_` record's first clip).
    pub tracks: BTreeMap<u16, RateTrack>,
    /// The first clip's duration in seconds, 0 without an `ANM_`.
    pub duration: f32,
}

/// Everything effect-related in one WAD.
#[derive(Default)]
pub struct Bank {
    pub shapes: Vec<Shape>,
    /// Emitter shape name (`PTC_` record `+0x54`) -> index into `shapes`.
    pub shape_index: HashMap<String, usize>,
    pub emitters: HashMap<String, Emitter>,
    pub geoms: HashMap<String, Geometry>,
    pub effects: Vec<EffectDef>,
}

impl Bank {
    /// Reads the shapes, emitters, geometry and effect groups of a WAD.
    pub fn load(wad_data: &[u8]) -> Bank {
        let recs: Vec<wad::Record> = wad::records(wad_data).collect();
        let mut first: HashMap<&str, &[u8]> = HashMap::new();
        for r in &recs {
            if r.tag == wad::Tag::Object && !r.data.is_empty() {
                first.entry(r.name.as_str()).or_insert(r.data);
            }
        }
        let mut bank = Bank::default();
        // a shape's material is the MAT_ reference right after it
        let mut mats: HashMap<&str, String> = HashMap::new();
        for w in recs.windows(2) {
            if w[0].tag == wad::Tag::Object && !w[0].data.is_empty() && w[0].name.starts_with("PTC_") && w[1].name.starts_with("MAT_") {
                mats.entry(w[0].name.as_str()).or_insert_with(|| w[1].name.clone());
            }
        }
        let mut names: Vec<&&str> = first.keys().collect();
        names.sort();
        for n in names {
            let b = first[*n];
            if n.starts_with("PTC_") {
                let s = Shape::parse(n, b, mats.get(*n).cloned());
                bank.shape_index.entry(s.shape_name.clone()).or_insert(bank.shapes.len());
                bank.shapes.push(s);
            }
        }
        // emission geometry
        let mut raw: HashMap<&str, &[u8]> = HashMap::new();
        for r in &recs {
            if !r.data.is_empty() && (r.name.starts_with("NCV_") || r.name.starts_with("MSH_")) {
                raw.entry(r.name.as_str()).or_insert(r.data);
            }
        }
        for (n, b) in &first {
            if !n.starts_with("FXC_") || b.len() < 0x88 || u16at(b, 2) != 13 {
                continue;
            }
            let (gname, rname, kind) = (cstr(b, 0x70, 24), cstr(b, 0x58, 24), u32at(b, 0x54) & 3);
            let Some(rb) = raw.get(rname.as_str()) else { continue };
            let mut matrix = [0.0f32; 16];
            for (i, m) in matrix.iter_mut().enumerate() {
                *m = f32at(b, 0x10 + 4 * i);
            }
            let kind = match kind {
                0 => {
                    let ns = u32at(rb, 0) as usize;
                    GeomKind::Curve { segs: (0..16 * ns).map(|i| f32at(rb, 0x10 + 4 * i)).collect(), knots: (0..ns).map(|i| f32at(rb, 0x10 + 0x40 * ns + 4 * i)).collect() }
                }
                1 => {
                    let (nv, nt) = (u32at(rb, 0) as usize, u32at(rb, 4) as usize);
                    GeomKind::Mesh { verts: (0..6 * nv).map(|i| f32at(rb, 0x10 + 4 * i)).collect(), tris: (0..4 * nt).map(|i| u16at(rb, 0x10 + 0x18 * nv + 2 * i)).collect() }
                }
                _ => continue,
            };
            bank.geoms.entry(gname).or_insert(Geometry { matrix, kind });
        }
        // emitters
        for (n, b) in &first {
            if !n.starts_with("FXC_") || matches!(u16at(b, 2), 11..=13) || b.len() < 0xE4 {
                continue;
            }
            let sub = u16at(b, 2);
            let mut matrix = [0.0f32; 16];
            for (i, m) in matrix.iter_mut().enumerate() {
                *m = f32at(b, 0x10 + 4 * i);
            }
            let mut p = [0.0f32; 24];
            for (i, v) in p.iter_mut().enumerate() {
                *v = f32at(b, 0x84 + 4 * i);
            }
            let geom = if sub == 3 || sub == 4 { Some(cstr(b, 0x6c, 24)).filter(|s| !s.is_empty()) } else { None };
            bank.emitters.insert(n.to_string(), Emitter { name: n.to_string(), subtype: sub, shape_name: cstr(b, 0x54, 24), matrix, joint: u16at(b, 8) as i16, channel: u16at(b, 0x0a), p, geom });
        }
        // effect groups: an unnamed object record with header (1, 1) followed by empty references
        let mut go: Option<String> = None;
        for (i, r) in recs.iter().enumerate() {
            if r.tag == wad::Tag::Object && r.name.to_lowercase().starts_with("go") && !r.data.is_empty() && u16at(r.data, 0) == 1 && u16at(r.data, 2) == 3 {
                go = Some(r.name.clone());
            }
            if r.tag != wad::Tag::Object || !r.name.is_empty() || r.data.len() < 0x28 || u16at(r.data, 0) != 1 || u16at(r.data, 2) != 1 {
                continue;
            }
            let (mut model, mut anm_name, mut fxc) = (None, None, Vec::new());
            for r2 in &recs[i + 1..] {
                if r2.tag != wad::Tag::Object || !r2.data.is_empty() {
                    break;
                }
                if let Some(m) = r2.name.strip_prefix("MDL_") {
                    model.get_or_insert(m.to_string());
                } else if r2.name.starts_with("ANM_") {
                    anm_name.get_or_insert(r2.name.clone());
                } else if r2.name.starts_with("FXC_") {
                    fxc.push(r2.name.clone());
                }
            }
            if fxc.is_empty() {
                continue;
            }
            let (tracks, duration) = anm_name.as_deref().and_then(|a| first.get(a)).map_or((BTreeMap::new(), 0.0), |a| rate_tracks(a));
            bank.effects.push(EffectDef { go: go.clone(), model, anm: anm_name, rig: r.data.to_vec(), emitters: fxc, tracks, duration });
        }
        bank
    }

    /// The effect that has `name` as its `go` node, or as its model, or whose first emitter is called so.
    pub fn effect(&self, name: &str) -> Option<usize> {
        let n = name.to_lowercase();
        self.effects.iter().position(|e| e.go.as_deref().is_some_and(|g| g.to_lowercase() == n) || e.model.as_deref().is_some_and(|m| m.to_lowercase() == n) || e.emitters.first().is_some_and(|f| f.to_lowercase() == n) || e.anm.as_deref().is_some_and(|a| a.to_lowercase() == n))
    }
}

/// The type-10 tracks of an `ANM_` record's first clip: block index -> rate keys (slot 8), and the clip's duration.
fn rate_tracks(a: &[u8]) -> (BTreeMap<u16, RateTrack>, f32) {
    let mut out = BTreeMap::new();
    let (nt, ng) = (u16at(a, 0x10) as usize, u16at(a, 0x12) as usize);
    let Some(&c) = anm::clips(a).first() else { return (out, 0.0) };
    let duration = f32at(a, c + 0x14);
    let mut blk = 0usize;
    for i in 0..nt {
        let o = 0x18 + 4 * ng + 8 * i;
        let (ttype, nsub) = (u16at(a, o), *a.get(o + 3).unwrap_or(&0) as usize);
        for _ in 0..nsub {
            if ttype == 10 {
                let b = c + 0x60 + 16 * blk;
                let (nseg, tab, dt) = (u16at(a, b + 2) as usize, u32at(a, b + 8) as usize, f32at(a, b + 12));
                let mut keys: BTreeMap<i64, f64> = BTreeMap::new();
                for s in 0..nseg {
                    if let Some(seg) = anm::decode_segment(a, c + tab + 12 * s, anm::Kind::Trans) {
                        if let Some(cv) = seg.curves.get(&8) {
                            keys.extend(cv.iter().map(|(&f, &v)| (f, v)));
                        }
                    }
                }
                out.insert(blk as u16, RateTrack { dt, duration, keys: keys.into_iter().map(|(f, v)| (f, v as f32)).collect() });
            }
            blk += 1;
        }
    }
    (out, duration)
}
