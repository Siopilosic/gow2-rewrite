//! Playing effects: emitters spawn particles, the particle programs are evaluated each frame, and the result is a list of sprites.
//!
//! A port of the playback of `analysis/levels/particles.js` (documented in `docs/particles.md` section 6), with the same emitter rules: directional and omni emitters
//! (subtypes 1 and 2), surface and curve emitters on a geometry (3 and 4) and the five volume emitters (5 to 9).
//!
//! Matrices are the game's row-vector 4 x 4 (`[f32; 16]`, a point is `p * M`); an emitter's world matrix is `local * joint * root`.

use std::collections::HashMap;

use gow2_formats::skin;
use gow2_skel::{mat_mul, transform_point, world_matrices, Clip, Mat4, Skeleton};

use crate::bank::{Bank, Blend, EffectDef, Emitter, GeomKind, Shape};

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len2(a: V3) -> f32 {
    dot(a, a)
}
fn norm(a: V3) -> V3 {
    let l = len2(a).sqrt();
    if l < 1e-9 {
        [0.0; 3]
    } else {
        scale(a, 1.0 / l)
    }
}
/// A velocity through the rotation of a row-vector matrix, without its scale. The effect rigs animate the scale of the joints their emitters hang under (`goearthstomp`'s
/// ring joint grows from 0.4 to 9 in 0.3 s so that the torus emitter's *position* spreads like the shock wave); a velocity scaled by that too would fling the debris
/// 4,000 units a second. MEDIUM: the game's velocity transform is not read; positions use the full matrix.
fn rotate_unit(m: &Mat4, v: V3) -> V3 {
    let col = |c: usize| {
        let l = (m[c] * m[c] + m[c + 1] * m[c + 1] + m[c + 2] * m[c + 2]).sqrt().max(1e-9);
        let _ = l;
        l
    };
    // the rows of the matrix are the images of the local axes: normalise each
    let (lx, ly, lz) = (col(0), col(4), col(8));
    [
        v[0] * m[0] / lx + v[1] * m[4] / ly + v[2] * m[8] / lz,
        v[0] * m[1] / lx + v[1] * m[5] / ly + v[2] * m[9] / lz,
        v[0] * m[2] / lx + v[1] * m[6] / ly + v[2] * m[10] / lz,
    ]
}

/// A direction through the rotation and scale of a row-vector matrix.
fn rotate_full(m: &Mat4, v: V3) -> V3 {
    [v[0] * m[0] + v[1] * m[4] + v[2] * m[8], v[0] * m[1] + v[1] * m[5] + v[2] * m[9], v[0] * m[2] + v[1] * m[6] + v[2] * m[10]]
}

/// Where an emitter is: positions go through the whole matrix `local * joint * root`; velocities keep the scale of the placement (`root`) but not the scale the rig
/// animates into its joints.
struct Frame {
    m: Mat4,
    lj: Mat4,
    root: Mat4,
    /// The joint's scale at rest (the blades' rig carries 1/64): velocities keep that, not the scale the clip animates.
    js: f32,
}

impl Frame {
    fn point(&self, p: V3) -> V3 {
        transform_point(&self.m, p)
    }
    fn dir(&self, v: V3) -> V3 {
        rotate_full(&self.root, scale(rotate_unit(&self.lj, v), self.js))
    }
}

/// A small random number generator (xorshift32), enough for particle spread.
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16777216.0
    }
}

/// One sprite for the front end: where, how big (the shape's own size unit, screen-relative, see [`SIZE_UNIT`]), turned by how much, and its colour.
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    /// Index into `Bank::shapes`.
    pub shape: usize,
    pub pos: V3,
    /// Half extent in the particle program's size unit; multiply by the front end's size scale.
    pub size: f32,
    pub angle: f32,
    /// Colour and alpha, 0 to 1 (the program's 128 is 1.0).
    pub rgba: [f32; 4],
    /// The colour of the rim of a disc (render routine 4: a gouraud fan from the centre colour to this one), zero for the other shapes.
    pub rim: [f32; 4],
}

/// The world half extent of a particle of size `s` is `s * SIZE_UNIT * tan(fov_x / 2)`, whatever its distance. The VU puts the half extent at `s * 2047.5 * Q` pixels from the
/// centre and scales it by the guard-band factor `vf19` of the batch; with `vf19 = 256 / 2048` (the viewport half width over the guard band, MEDIUM: the batch constant is a runtime
/// field) that is `s * 256 / w` pixels, which on a frame of 256 pixels half width is `s * tan_x` world units: 0.514 with the game's own field of view. The first port took the
/// size in pixels (4.11, far too big), the second in 1/16 pixel (0.257); the glow of the blades' flames in a PCSX2 frame (`flame3`, size 4 to 7) and the lightning on the arm
/// bones (size up to 10 on a 5.5-unit bone) rule the first out and sit between the other two (`docs/particles.md` 7.1).
pub const SIZE_UNIT: f32 = 1.0;

struct Particle {
    d: Vec<f32>,
    birth: f32,
    /// The scale of the placement the particle was born under: sizes follow it, so a half-size effect has half-size puffs.
    scale: f32,
}

struct EmitterState {
    shape: Option<usize>,
    carry: f32,
    rng: Rng,
}

/// A running effect: an emitter set with a clock, placed in the world.
pub struct Instance {
    /// A number that is unique among the effects this player ever started.
    pub serial: u64,
    pub def: usize,
    /// Row-vector world placement of the effect's origin (scale included).
    pub root: Mat4,
    pub age: f32,
    /// Emission stops after this many seconds (particles already out live on).
    pub duration: f32,
    states: Vec<EmitterState>,
    /// Emitters that may emit (all when empty): a filter of the effect's emitters by name prefix.
    allowed: Vec<bool>,
    done: bool,
}

/// A skeleton and clip for an effect that has a model (`goearthstomp`): the joints its emitters hang under.
struct Rig {
    skel: Skeleton,
    /// The first clip when it has transform tracks (the effect rigs that only animate emitter rates have none: their joints stay in the bind pose).
    clip: Option<Clip>,
}

pub struct Player {
    pub bank: Bank,
    pub instances: Vec<Instance>,
    parts: Vec<Vec<Particle>>,
    rigs: HashMap<usize, Option<Rig>>,
    pub time: f32,
    seed: u32,
    serials: u64,
}

const MAX_PER_SHAPE: usize = 1500;

impl Player {
    pub fn new(bank: Bank) -> Player {
        let n = bank.shapes.len();
        Player { bank, instances: Vec::new(), parts: (0..n).map(|_| Vec::new()).collect(), rigs: HashMap::new(), time: 0.0, seed: 0x9E37_79B9, serials: 0 }
    }

    /// Joint matrices of the effect's rig at `t` seconds, or `None` for the effects without a model (their emitters sit at joint 0 = the origin).
    pub fn joints(&mut self, def: usize, anm_data: Option<&[u8]>, t: f32) -> Option<Vec<Mat4>> {
        let _ = anm_data;
        let rig = self.rigs.get(&def)?.as_ref()?;
        let local = match &rig.clip {
            Some(c) => c.sample(&rig.skel, t.min(c.duration.max(1e-3))),
            None => rig.skel.bind.clone(),
        };
        Some(world_matrices(&rig.skel, &local))
    }

    /// The scale of a rig joint at rest (the length of its x axis in the bind pose), 1 without a rig.
    fn rest_scale(&self, def: usize, joint: usize) -> f32 {
        self.rigs.get(&def).and_then(|r| r.as_ref()).and_then(|r| r.skel.bind_world.get(joint)).map_or(1.0, |m| (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt())
    }

    /// Attaches the skeleton and first clip of the effects that have an `MDL_`: `wad` is the data of the WAD the bank came from.
    pub fn attach_rigs(&mut self, wad_data: &[u8]) {
        let recs: Vec<gow2_formats::wad::Record> = gow2_formats::wad::records(wad_data).collect();
        let rigs = skin::find_rigs(&recs);
        let mut first: HashMap<&str, &[u8]> = HashMap::new();
        for r in &recs {
            if r.tag == gow2_formats::wad::Tag::Object && !r.data.is_empty() {
                first.entry(r.name.as_str()).or_insert(r.data);
            }
        }
        for (i, e) in self.bank.effects.iter().enumerate() {
            let built = e.model.as_ref().and_then(|m| rigs.iter().find(|r| &r.model == m)).and_then(|rr| {
                let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
                let clip = rr.anm.as_deref().and_then(|n| first.get(n)).and_then(|anm| skin::clip_channels(anm, skel.len() as u32, None)).map(|cc| Clip::bake(&cc, &skel));
                Some(Rig { skel, clip })
            });
            self.rigs.insert(i, built);
        }
    }

    /// Starts the effect `name` (its `go` node, model or first emitter) placed by `root`. Returns false when the bank has no such effect.
    pub fn start(&mut self, name: &str, root: Mat4, duration: Option<f32>) -> bool {
        self.start_serial(name, root, duration).is_some()
    }

    /// Like [`Player::start`], returning the new instance's serial number.
    pub fn start_serial(&mut self, name: &str, root: Mat4, duration: Option<f32>) -> Option<u64> {
        self.start_filtered(name, root, duration, &[])
    }

    /// Like [`Player::start_serial`], but only the emitters whose names start with one of `prefixes` emit (all of them when the list is empty).
    pub fn start_filtered(&mut self, name: &str, root: Mat4, duration: Option<f32>, prefixes: &[&str]) -> Option<u64> {
        let def = self.bank.effect(name)?;
        let e = &self.bank.effects[def];
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let seed = self.seed;
        let states = e
            .emitters
            .iter()
            .enumerate()
            .map(|(k, n)| EmitterState { shape: self.bank.emitters.get(n).and_then(|em| self.bank.shape_index.get(&em.shape_name).copied()), carry: 0.0, rng: Rng::new(seed ^ (k as u32 + 1).wrapping_mul(0x85EB_CA6B)) })
            .collect();
        // an effect with a clip runs as long as the clip; the ones without (continuous emitters) as long as the caller says, a second by default
        let duration = duration.unwrap_or(if e.duration > 0.0 { e.duration } else { 1.0 });
        self.serials += 1;
        let serial = self.serials;
        let allowed = e.emitters.iter().map(|n| prefixes.is_empty() || prefixes.iter().any(|p| n.starts_with(p))).collect();
        self.instances.push(Instance { serial, def, root, age: 0.0, duration, states, allowed, done: false });
        Some(serial)
    }

    /// Moves the effect started last with `name`... for effects that follow something: sets the root of instance `i`.
    pub fn set_root(&mut self, i: usize, root: Mat4) {
        if let Some(inst) = self.instances.get_mut(i) {
            inst.root = root;
        }
    }

    /// Ends the emission of instance `i` (its particles live on).
    pub fn stop(&mut self, i: usize) {
        if let Some(inst) = self.instances.get_mut(i) {
            inst.duration = inst.age;
        }
    }

    /// Live particles (for tests and statistics).
    pub fn live(&self) -> usize {
        self.parts.iter().map(|p| p.len()).sum()
    }

    /// Steps the clock: emitters spawn, old particles die.
    pub fn update(&mut self, dt: f32) {
        let dt = dt.min(0.1);
        self.time += dt;
        let time = self.time;
        for ii in 0..self.instances.len() {
            let (def, age, duration, root) = {
                let i = &self.instances[ii];
                (i.def, i.age, i.duration, i.root)
            };
            // an effect that is told to run on (the blades' flames) repeats its clip and its rate tracks
            let clip_len = self.bank.effects[def].duration;
            let age = if duration > 100.0 && clip_len > 0.0 { age % clip_len } else { age };
            let joints = self.joints(def, None, age);
            let emitters: Vec<String> = self.bank.effects[def].emitters.clone();
            for (k, name) in emitters.iter().enumerate() {
                if !self.instances[ii].allowed.get(k).copied().unwrap_or(true) {
                    continue;
                }
                let Some(em) = self.bank.emitters.get(name).cloned() else { continue };
                let rate = if age > duration {
                    0.0
                } else {
                    match self.bank.effects[def].tracks.get(&em.channel) {
                        Some(tr) => tr.at(age),
                        None => em.rate(),
                    }
                };
                if rate <= 0.0 {
                    continue;
                }
                let st = &mut self.instances[ii].states[k];
                let n = rate * dt + st.carry;
                let c = n.floor();
                st.carry = n - c;
                let Some(shape) = st.shape else { continue };
                let joint_m = joints.as_ref().and_then(|j| if em.joint >= 0 { j.get(em.joint as usize).copied() } else { None });
                // the world matrix: the emitter's local matrix (or its geometry's), then its joint, then the placement
                let geom = em.geom.as_ref().and_then(|g| self.bank.geoms.get(g));
                let local = geom.map_or(em.matrix, |g| g.matrix);
                let lj = mat_mul(&local, &joint_m.unwrap_or(IDENTITY));
                let js = if em.joint >= 0 { self.rest_scale(def, em.joint as usize) } else { 1.0 };
                let frame = Frame { m: mat_mul(&lj, &root), lj, root, js };
                for _ in 0..(c as usize) {
                    if self.parts[shape].len() >= MAX_PER_SHAPE {
                        break;
                    }
                    let rng = &mut self.instances[ii].states[k].rng;
                    let p = spawn(&em, geom.map(|g| &g.kind), &frame, rng);
                    let sh = &self.bank.shapes[shape];
                    let d = init_data(sh, p, time, rng);
                    let scale = (root[0] * root[0] + root[1] * root[1] + root[2] * root[2]).sqrt();
                    self.parts[shape].push(Particle { d, birth: time, scale });
                }
            }
            let inst = &mut self.instances[ii];
            inst.age += dt;
            let tail = self.bank.effects[def].emitters.iter().filter_map(|n| self.bank.emitters.get(n)).filter_map(|e| self.bank.shape_index.get(&e.shape_name)).map(|&s| self.bank.shapes[s].life.max(0.0)).fold(0.0, f32::max);
            if inst.age > inst.duration + tail + 0.1 {
                inst.done = true;
            }
        }
        self.instances.retain(|i| !i.done);
        for (s, parts) in self.parts.iter_mut().enumerate() {
            let life = if self.bank.shapes[s].life > 0.0 { self.bank.shapes[s].life } else { 1e9 };
            parts.retain(|p| time - p.birth <= life);
        }
    }

    /// The sprites of this frame, grouped by shape in `Bank::shapes` order.
    pub fn sprites(&self) -> Vec<Sprite> {
        let mut out = Vec::new();
        for (si, parts) in self.parts.iter().enumerate() {
            let sh = &self.bank.shapes[si];
            let l = &sh.lists;
            for p in parts {
                let age = self.time - p.birth;
                let mut t = [[0.0f32; 4]; 4];
                for (i, &opc) in l[6].iter().enumerate() {
                    let o = eval_op(opc, &p.d, *l[7].get(i).unwrap_or(&0) as usize, age);
                    if i < l[4].len() {
                        if let Some(slot) = t.get_mut(l[4][i] as usize) {
                            *slot = o;
                        }
                    } else if let Some(&j) = l[5].get(i - l[4].len()) {
                        if let Some(slot) = t.get_mut(j as usize) {
                            slot[3] = o[3];
                        }
                    }
                }
                // the template per render routine (docs/particles.md 3.3)
                let (mut rgba, mut c, mut size, mut ang) = (t[0], t[1], t[1][3], if sh.render == 3 { t[2][3] } else { 0.0 });
                if sh.render == 4 {
                    c = t[2];
                    size = t[2][3];
                }
                if sh.render == 7 {
                    rgba = t[1];
                    c = t[2];
                    size = 2.0;
                }
                if sh.render == 0 {
                    size = 2.0;
                }
                if !(sh.render == 3 || sh.render == 4 || sh.render == 7 || sh.render == 0) {
                    ang = 0.0;
                }
                out.push(Sprite { shape: si, pos: [c[0], c[1], c[2]], size: size.abs() * p.scale, angle: ang, rgba: [rgba[0] / 128.0, rgba[1] / 128.0, rgba[2] / 128.0, (rgba[3] / 128.0).clamp(0.0, 1.0)], rim: if sh.render == 4 { [t[1][0] / 128.0, t[1][1] / 128.0, t[1][2] / 128.0, (t[1][3] / 128.0).clamp(0.0, 1.0)] } else { [0.0; 4] } });
            }
        }
        out
    }

    pub fn shape(&self, i: usize) -> &Shape {
        &self.bank.shapes[i]
    }

    pub fn effect_def(&self, i: usize) -> &EffectDef {
        &self.bank.effects[i]
    }

    pub fn blend(&self, shape: usize) -> Blend {
        self.bank.shapes[shape].blend()
    }
}

const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Position and velocity of a new particle in world space.
fn spawn(e: &Emitter, geom: Option<&GeomKind>, f: &Frame, r: &mut Rng) -> (V3, V3) {
    let spd = (e.speed() - e.speed_range() / 2.0 + r.next() * e.speed_range()) * 16.0;
    let rad = (e.radius()[0] + (e.radius()[1] - e.radius()[0]) * r.next()) * 16.0;
    let pp = &e.p;
    // a cone around an axis (FUN_00133970: polar = rand * spread * pi / 2, azimuth = rand * 2 pi)
    let cone = |axis: V3, r: &mut Rng| -> V3 {
        let th = r.next() * e.spread() * std::f32::consts::FRAC_PI_2;
        let ph = r.next() * std::f32::consts::TAU;
        let a = norm(axis);
        let mut t1 = cross([0.0, 1.0, 0.0], a);
        if len2(t1) < 1e-4 {
            t1 = cross([1.0, 0.0, 0.0], a);
        }
        let t1 = norm(t1);
        let t2 = cross(a, t1);
        add(add(scale(a, th.cos()), scale(t1, th.sin() * ph.sin())), scale(t2, th.sin() * ph.cos()))
    };
    let rnd3 = |r: &mut Rng| [r.next() - 0.5, r.next() - 0.5, r.next() - 0.5];
    match geom {
        Some(GeomKind::Curve { segs, knots }) => {
            // FUN_00135528: a random global parameter, P(t) = [t^3 t^2 t 1] . M_i / w; tangent [3t^2 2t 1 0] . M_i
            let n = knots.len();
            let t = r.next() * 0.99999 * knots.get(n.wrapping_sub(1)).copied().unwrap_or(1.0);
            let mut i = 0;
            if n > 1 && knots[0] < t {
                i = 1;
                while i < n - 1 && knots[i] < t {
                    i += 1;
                }
            }
            let ev = |w: [f32; 4]| -> [f32; 4] {
                let mut o = [0.0; 4];
                for (c, v) in o.iter_mut().enumerate() {
                    *v = w[0] * segs[i * 16 + c] + w[1] * segs[i * 16 + 4 + c] + w[2] * segs[i * 16 + 8 + c] + w[3] * segs[i * 16 + 12 + c];
                }
                o
            };
            let q = ev([t * t * t, t * t, t, 1.0]);
            let mut pos = [q[0] / q[3], q[1] / q[3], q[2] / q[3]];
            let tq = ev([3.0 * t * t, 2.0 * t, 1.0, 0.0]);
            let tan = [tq[0], tq[1], tq[2]];
            let mut dir = [0.0; 3];
            if (pp[9] > 0.0 || pp[10] > 0.0) && len2(tan) > 1e-12 {
                if pp[10] > 0.0 {
                    let nrm = scale(norm(cross(tan, rnd3(r))), pp[10]);
                    dir = norm(add(nrm, scale(norm(tan), pp[9])));
                } else {
                    dir = norm(tan);
                }
            }
            if len2(dir) > 0.0 {
                pos = add(pos, scale(dir, rad));
            }
            let d = if e.spread() > 0.0 && len2(dir) > 0.0 { cone(dir, r) } else { dir };
            (f.point(pos), f.dir(scale(d, spd)))
        }
        Some(GeomKind::Mesh { verts, tris }) => {
            // FUN_00135e48: a triangle by the cumulative-area table, a uniform barycentric point, the interpolated normal
            let key = r.next() * 65535.0;
            let nt = tris.len() / 4;
            let mut k = 0;
            while k + 1 < nt && (tris[k * 4 + 3] as f32) < key {
                k += 1;
            }
            let (mut a, mut b) = (r.next(), r.next());
            if a + b > 1.0 {
                a = 1.0 - a;
                b = 1.0 - b;
            }
            let w = [a, b, 1.0 - a - b];
            let (mut pos, mut nrm) = ([0.0; 3], [0.0; 3]);
            for (j, wj) in w.iter().enumerate() {
                let o = tris[k * 4 + j] as usize * 6;
                if o + 5 < verts.len() {
                    pos = add(pos, scale([verts[o], verts[o + 1], verts[o + 2]], *wj));
                    nrm = add(nrm, scale([verts[o + 3], verts[o + 4], verts[o + 5]], *wj));
                }
            }
            let nrm = norm(nrm);
            let mut d = [0.0; 3];
            if pp[10] != 0.0 {
                d = add(d, scale(nrm, pp[10]));
            }
            if pp[9] != 0.0 {
                d = add(d, scale(norm(cross(nrm, rnd3(r))), pp[9]));
            }
            if len2(d) > 0.0 {
                d = norm(d);
                pos = add(pos, scale(d, rad));
            }
            (f.point(pos), f.dir(scale(d, spd)))
        }
        None if (5..=9).contains(&e.subtype) => {
            // the volume emitters (docs/particles.md 4.1): Maya's volume-emitter parameters on a unit shape around the local Y axis, scaled by 16
            let (sweep, sect) = (pp[14], pp[15]);
            let az = r.next() * sweep;
            let q: V3 = match e.subtype {
                6 => {
                    let ct = 2.0 * r.next() - 1.0;
                    let st = (1.0 - ct * ct).max(0.0).sqrt();
                    let rr = r.next().cbrt();
                    [st * az.cos() * rr, ct * rr, st * az.sin() * rr]
                }
                7 => {
                    let rr = r.next().sqrt();
                    [az.cos() * rr, 2.0 * r.next() - 1.0, az.sin() * rr]
                }
                8 => {
                    let h = r.next().sqrt();
                    let rr = r.next().sqrt() * h;
                    [az.cos() * rr, h, az.sin() * rr]
                }
                9 => {
                    let b = r.next() * std::f32::consts::TAU;
                    let rr = r.next().sqrt() * sect;
                    let big = 1.0 + rr * b.cos();
                    [az.cos() * big, rr * b.sin(), az.sin() * big]
                }
                _ => {
                    let c = rnd3(r);
                    scale(c, 2.0)
                }
            };
            let mut vel = [0.0; 3];
            if len2(q) > 1e-8 {
                vel = add(vel, scale(norm(q), pp[16]));
            }
            let off = [q[0], 0.0, q[2]];
            if len2(off) > 1e-8 {
                vel = add(vel, scale(norm(off), pp[17]));
                vel = add(vel, scale(norm(cross([0.0, 1.0, 0.0], off)), pp[19]));
            }
            vel[1] += pp[18];
            if pp[20] != 0.0 {
                vel = add(vel, scale(rnd3(r), 2.0 * pp[20]));
            }
            let dirv = [pp[0], pp[1], pp[2]];
            if pp[21] != 0.0 && len2(dirv) > 1e-8 {
                vel = add(vel, scale(norm(dirv), pp[21]));
            }
            (f.point(scale(q, 16.0)), f.dir(scale(vel, spd)))
        }
        None => {
            let dir = if e.subtype == 2 {
                // omni (FUN_00133bd8): the normalised random point of the cube [-0.5, 0.5]^3, no spread
                let mut d = rnd3(r);
                while len2(d) < 1e-6 {
                    d = rnd3(r);
                }
                norm(d)
            } else {
                let ax = [pp[0], pp[1], pp[2]];
                cone(if len2(ax) > 1e-8 { ax } else { [1.0, 0.0, 0.0] }, r)
            };
            (f.point(scale(dir, rad)), f.dir(scale(dir, spd)))
        }
    }
}

/// The particle's data table: the shape's table with the spawn state and the random ranges written in (the game does this once, in VU memory).
fn init_data(sh: &Shape, state: (V3, V3), time: f32, r: &mut Rng) -> Vec<f32> {
    let mut d: Vec<f32> = sh.data.iter().flat_map(|q| q.iter().copied()).collect();
    let (p, v) = state;
    let st = [[p[0], p[1], p[2], time], [v[0], v[1], v[2], 0.0]];
    let l = &sh.lists;
    let set = |d: &mut Vec<f32>, idx: usize, k: usize, v: f32| {
        if let Some(x) = d.get_mut(idx * 4 + k) {
            *x = v;
        }
    };
    for (i, &dst) in l[0].iter().enumerate() {
        if let Some(s) = st.get(i) {
            for (k, v) in s.iter().enumerate() {
                set(&mut d, dst as usize, k, *v);
            }
        }
    }
    for (i, &dst) in l[1].iter().enumerate() {
        if let Some(s) = st.get(i) {
            set(&mut d, dst as usize, 3, s[3]);
        }
    }
    // the (base, range) pairs start at data entry 1
    let mut pair = 1usize;
    let at = |d: &Vec<f32>, e: usize, k: usize| d.get(e * 4 + k).copied().unwrap_or(0.0);
    for &dst in &l[2] {
        for k in 0..4 {
            let v = at(&d, pair, k) + at(&d, pair + 1, k) * r.next();
            set(&mut d, dst as usize, k, v);
        }
        pair += 2;
    }
    for &dst in &l[3] {
        let v = at(&d, pair, 3) + at(&d, pair + 1, 3) * r.next();
        set(&mut d, dst as usize, 3, v);
        pair += 2;
    }
    d
}

/// One program operator at the particle's age (docs/particles.md 3.1; the ones not decoded fall back to their first operand).
fn eval_op(code: u16, d: &[f32], a: usize, age: f32) -> [f32; 4] {
    let q = |j: usize, k: usize| d.get((a + j) * 4 + k).copied().unwrap_or(0.0);
    let h = age * age / 2.0;
    let mut out = [0.0; 4];
    for (k, o) in out.iter_mut().enumerate() {
        *o = match code {
            2 | 11 => q(0, k) + q(1, k) * age,
            3 => (q(0, k) + q(1, k) * age).clamp(0.0, 255.0),
            4 | 5 | 12 => q(0, k) + q(1, k) * age + q(2, k) * h,
            16 => q(1, k) * q(0, k),
            17 => (q(1, k) + q(2, k) * age) * q(0, k),
            18 => (q(1, k) + q(2, k) * age + q(3, k) * h) * q(0, k),
            _ => q(0, k),
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operators_follow_the_documented_formulas() {
        let d = [1.0, 2.0, 3.0, 4.0, 10.0, 20.0, 30.0, 40.0, 100.0, 100.0, 100.0, 100.0];
        // linear: q0 + q1 * age
        assert_eq!(eval_op(2, &d, 0, 0.5), [6.0, 12.0, 18.0, 24.0]);
        // clamped colour
        // (operand 0 is entry 1 = 10, operand 1 is entry 2 = 100)
        assert_eq!(eval_op(3, &d, 1, 0.5)[0], 60.0);
        assert_eq!(eval_op(3, &d, 1, 100.0)[0], 255.0);
        // ballistic: q0 + q1 * age + q2 * age^2 / 2
        assert_eq!(eval_op(4, &d, 0, 2.0)[0], 1.0 + 10.0 * 2.0 + 100.0 * 2.0);
        // constant
        assert_eq!(eval_op(0, &d, 0, 9.0), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_rate_track_is_linear_between_keys_and_zero_outside() {
        let t = crate::bank::RateTrack { dt: 0.1, duration: 1.0, keys: vec![(0, 0.0), (2, 100.0), (4, 0.0)] };
        assert_eq!(t.at(0.0), 0.0);
        assert!((t.at(0.1) - 50.0).abs() < 1e-3);
        assert!((t.at(0.2) - 100.0).abs() < 1e-3);
        assert_eq!(t.at(0.9), 0.0);
    }

    #[test]
    fn the_random_numbers_stay_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let x = r.next();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
