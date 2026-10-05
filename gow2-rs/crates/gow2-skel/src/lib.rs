//! Skeletons, baked clips and CPU skinning for God of War II characters.
//!
//! No engine types. Matrices are 4x4 row-major in the game's row-vector convention: `p' = p * M`, translation
//! in row 3, `world = local * parent_world`. The glTF exporter (`tools/gltf_export.py`) is the reference for
//! how channels are applied: each joint starts from its rig TRS, a clip key overwrites the components it
//! names and keeps the others, rotations are stored as 16384 = 1.0 and normalised, and keys interpolate
//! linearly in time (rotation as a normalised blend).

use gow2_formats::skin::{ClipChannels, Rig};

pub type Mat4 = [f32; 16];

/// Length of a unit quaternion in clip units. Measured on Kratos's clips: full four-component samples have length
/// about 20,860 = 8 * 16384 / (2 pi), not 16384. Components a clip does not animate come from the bind pose at this scale.
pub const ROT_UNIT: f64 = 8.0 * 16384.0 / (2.0 * std::f64::consts::PI);

pub const IDENTITY: Mat4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            o[4 * i + j] = (0..4).map(|k| a[4 * i + k] * b[4 * k + j]).sum();
        }
    }
    o
}

/// Gauss-Jordan inverse with partial pivoting (same method as the Python exporter).
pub fn mat_inv(m: &Mat4) -> Mat4 {
    let mut a = [[0f64; 8]; 4];
    for i in 0..4 {
        for j in 0..4 {
            a[i][j] = m[4 * i + j] as f64;
        }
        a[i][4 + i] = 1.0;
    }
    for c in 0..4 {
        let p = (c..4).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs())).unwrap();
        a.swap(c, p);
        let d = if a[c][c] == 0.0 { 1e-12 } else { a[c][c] };
        for x in a[c].iter_mut() {
            *x /= d;
        }
        for r in 0..4 {
            if r != c {
                let f = a[r][c];
                let row = a[c];
                for (x, y) in a[r].iter_mut().zip(row.iter()) {
                    *x -= f * y;
                }
            }
        }
    }
    let mut o = [0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            o[4 * i + j] = a[i][4 + j] as f32;
        }
    }
    o
}

/// Translation, rotation (quaternion x y z w) and scale of a joint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trs {
    pub t: [f64; 3],
    pub q: [f64; 4],
    pub s: [f64; 3],
}

impl Trs {
    /// Row-vector matrix: row i = s_i * (row i of the transposed column-vector rotation), translation in row 3.
    pub fn to_mat(&self) -> Mat4 {
        let [x, y, z, w] = self.q;
        let n = (x * x + y * y + z * z + w * w).sqrt();
        let n = if n == 0.0 { 1.0 } else { n };
        let (x, y, z, w) = (x / n, y / n, z / n, w / n);
        // column-vector rotation matrix R[i][j]
        let r = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        let mut m = IDENTITY;
        for i in 0..3 {
            for j in 0..3 {
                m[4 * i + j] = (self.s[i] * r[j][i]) as f32;
            }
            m[12 + i] = self.t[i] as f32;
        }
        m
    }
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<i16>,
    pub bind: Vec<Trs>,
    pub bind_world: Vec<Mat4>,
    pub inv_bind: Vec<Mat4>,
}

impl Skeleton {
    pub fn from_rig(rig: &Rig) -> Self {
        let bind: Vec<Trs> = rig
            .mats
            .iter()
            .map(|m| {
                let (t, q, s) = gow2_formats::skin::mat_to_trs(m);
                Trs { t, q, s }
            })
            .collect();
        // world bind matrices come from the stored local matrices (the Python exporter's `world` list)
        let mut bind_world: Vec<Mat4> = Vec::with_capacity(rig.mats.len());
        for (j, m) in rig.mats.iter().enumerate() {
            let p = rig.parents[j];
            bind_world.push(if p >= 0 && (p as usize) < j { mat_mul(m, &bind_world[p as usize]) } else { *m });
        }
        let inv_bind = bind_world.iter().map(mat_inv).collect();
        Skeleton { names: rig.names.clone(), parents: rig.parents.clone(), bind, bind_world, inv_bind }
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }
}

/// Keys of one channel: frame numbers and the value at each (rotation already normalised).
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub frames: Vec<i64>,
    pub values: Vec<[f64; 4]>,
}

#[derive(Debug, Clone, Default)]
pub struct JointTracks {
    /// The rotation channel stores fewer than four components (so it is a delta on the bind rotation; see Delta).
    pub rot_partial: bool,
    pub rot: Option<Track>,
    pub trans: Option<Track>,
    pub scale: Option<Track>,
}

/// How rotation components a clip does not animate are filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// From the bind quaternion (the Python exporter's rule).
    Bind,
    /// Missing x, y, z are zero; a missing w is derived from the unit length (sqrt(U^2 - x^2 - y^2 - z^2)).
    Implicit,
}

/// How a rotation channel with fewer than four stored components combines with the bind rotation (HIGH that it is a
/// delta, from the per-joint statistics in docs/animation.md; the order is tested on screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delta {
    /// The clip rotation replaces the bind rotation (the old rule).
    Off,
    /// R_row(bind) * R_row(delta): the delta acts after the bind rotation in the row-vector chain.
    BindThenDelta,
    /// R_row(delta) * R_row(bind).
    DeltaThenBind,
}

/// Hamilton product  * b of quaternions (x, y, z, w).
pub fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    qmul_impl(a, b)
}

/// Radians per stored unit: 2 pi / 16384 (the constant 0.0003834952 in the game's matrix routine FUN_0010ab90).
pub const RAD_PER_UNIT: f64 = 2.0 * std::f64::consts::PI / 16384.0;

/// Rotation vector (axis * angle, radians) to a quaternion (x, y, z, w).
pub fn expmap(v: [f64; 3]) -> [f64; 4] {
    let th = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if th < 1e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let s = (th / 2.0).sin() / th;
    [v[0] * s, v[1] * s, v[2] * s, (th / 2.0).cos()]
}

fn qmul_impl(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// A clip baked into per-joint tracks, ready to sample.
#[derive(Debug, Clone)]
pub struct Clip {
    pub dt: f32,
    pub duration: f32,
    pub joints: Vec<JointTracks>,
}

impl Clip {
    /// Bakes decoded channels against the skeleton's bind TRS (the exporter's `add_animation`).
    pub fn bake(cc: &ClipChannels, skel: &Skeleton) -> Clip {
        Self::bake_with(cc, skel, Fill::Bind)
    }

    pub fn bake_with(cc: &ClipChannels, skel: &Skeleton, fill: Fill) -> Clip {
        let mut joints = vec![JointTracks::default(); skel.len()];
        for (&j, kinds) in &cc.joints {
            let j = j as usize;
            let b = skel.bind[j];
            for (ki, comps) in kinds.iter().enumerate() {
                let mut frames: Vec<i64> = comps.values().flat_map(|cv| cv.keys().copied()).collect();
                frames.sort_unstable();
                frames.dedup();
                if frames.is_empty() {
                    continue;
                }
                // A rotation channel that stores fewer than four components is a rotation vector (axis * angle, radians
                // once multiplied by 2 pi / 16384); a four-component channel is a quaternion. Verified against the game's
                // runtime joint rotations (docs/animation.md, "Ground truth from RAM").
                let partial_rot = ki == 0 && comps.len() < 4;
                let mut cur: [f64; 4] = match ki {
                    0 if partial_rot => [0.0; 4],
                    0 if fill == Fill::Implicit => [0.0, 0.0, 0.0, ROT_UNIT],
                    0 => [b.q[0] * ROT_UNIT, b.q[1] * ROT_UNIT, b.q[2] * ROT_UNIT, b.q[3] * ROT_UNIT],
                    1 => [b.t[0], b.t[1], b.t[2], 0.0],
                    _ => [b.s[0], b.s[1], b.s[2], 0.0],
                };
                let width = if ki == 0 { 4 } else { 3 };
                let mut values = Vec::with_capacity(frames.len());
                for &f in &frames {
                    for (&c, cv) in comps {
                        if let Some(&v) = cv.get(&f) {
                            if (c as usize) < width {
                                cur[c as usize] = v;
                            }
                        }
                    }
                    let mut v = cur;
                    if partial_rot {
                        // keep the rotation vector in radians; it is turned into a quaternion at sampling time
                        v = [v[0] * RAD_PER_UNIT, v[1] * RAD_PER_UNIT, v[2] * RAD_PER_UNIT, 0.0];
                        values.push(v);
                        continue;
                    }
                    if ki == 0 && fill == Fill::Implicit && !comps.contains_key(&3) {
                        let l2 = ROT_UNIT * ROT_UNIT - (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
                        v[3] = l2.max(0.0).sqrt();
                    }
                    if ki == 0 {
                        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3]).sqrt();
                        let n = if n == 0.0 { 1.0 } else { n };
                        v = [v[0] / n, v[1] / n, v[2] / n, v[3] / n];
                    }
                    values.push(v);
                }
                let t = Track { frames, values };
                if ki == 0 {
                    joints[j].rot_partial = comps.len() < 4;
                }
                match ki {
                    0 => joints[j].rot = Some(t),
                    1 => joints[j].trans = Some(t),
                    _ => joints[j].scale = Some(t),
                }
            }
        }
        Clip { dt: cc.dt, duration: cc.duration, joints }
    }

    /// Joint local TRS at `time` seconds (clamped to the clip), starting from the bind pose.
    pub fn sample(&self, skel: &Skeleton, time: f32) -> Vec<Trs> {
        self.sample_with(skel, time, Delta::Off)
    }

    pub fn sample_with(&self, skel: &Skeleton, time: f32, delta: Delta) -> Vec<Trs> {
        let frame = (time.clamp(0.0, self.duration.max(0.0)) / self.dt.max(1e-6)) as f64;
        skel.bind
            .iter()
            .zip(&self.joints)
            .map(|(b, jt)| {
                let mut o = *b;
                if let Some(tr) = &jt.rot {
                    if jt.rot_partial {
                        // rotation vector: interpolate the three numbers, then exp-map (the `delta` argument is a legacy
                        // experiment switch and no longer changes anything)
                        let _ = delta;
                        let v = tr.at(frame, false);
                        o.q = expmap([v[0], v[1], v[2]]);
                    } else {
                        o.q = tr.at(frame, true);
                    }
                }
                if let Some(tr) = &jt.trans {
                    let v = tr.at(frame, false);
                    o.t = [v[0], v[1], v[2]];
                }
                if let Some(tr) = &jt.scale {
                    let v = tr.at(frame, false);
                    o.s = [v[0], v[1], v[2]];
                }
                o
            })
            .collect()
    }
}

impl Track {
    /// Linear sample at a fractional frame, clamped at both ends. Rotations blend along the shorter arc and renormalise.
    pub fn at(&self, frame: f64, quat: bool) -> [f64; 4] {
        let n = self.frames.len();
        let i = self.frames.partition_point(|&f| (f as f64) <= frame);
        if i == 0 {
            return self.values[0];
        }
        if i >= n {
            return self.values[n - 1];
        }
        let (f0, f1) = (self.frames[i - 1] as f64, self.frames[i] as f64);
        let u = if f1 > f0 { (frame - f0) / (f1 - f0) } else { 0.0 };
        let (a, mut b) = (self.values[i - 1], self.values[i]);
        if quat && a.iter().zip(&b).map(|(x, y)| x * y).sum::<f64>() < 0.0 {
            b = [-b[0], -b[1], -b[2], -b[3]];
        }
        let mut o = [0.0; 4];
        for k in 0..4 {
            o[k] = a[k] + (b[k] - a[k]) * u;
        }
        if quat {
            let n = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2] + o[3] * o[3]).sqrt();
            if n > 0.0 {
                o.iter_mut().for_each(|x| *x /= n);
            }
        }
        o
    }
}

/// World matrices of every joint for a set of local TRS.
pub fn world_matrices(skel: &Skeleton, local: &[Trs]) -> Vec<Mat4> {
    let mut world: Vec<Mat4> = Vec::with_capacity(local.len());
    for (j, l) in local.iter().enumerate() {
        let m = l.to_mat();
        let p = skel.parents[j];
        world.push(if p >= 0 && (p as usize) < j { mat_mul(&m, &world[p as usize]) } else { m });
    }
    world
}

/// Skin matrix per joint: a bind-space vertex goes through `inv_bind * world`.
pub fn skin_matrices(skel: &Skeleton, world: &[Mat4]) -> Vec<Mat4> {
    world.iter().zip(&skel.inv_bind).map(|(w, ib)| mat_mul(ib, w)).collect()
}

/// Transforms a point (row vector, w = 1).
pub fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
    ]
}

/// Weight-1 CPU skinning of bind-space positions by one joint per vertex.
pub fn skin_positions(bind_pos: &[[f32; 3]], joints: &[i64], skin: &[Mat4]) -> Vec<[f32; 3]> {
    bind_pos
        .iter()
        .zip(joints)
        .map(|(p, &j)| match skin.get(j as usize) {
            Some(m) => transform_point(m, *p),
            None => *p,
        })
        .collect()
}


/// Which model parts hold joint-local vertices. Most of Kratos is modelled in bind-pose model space, but some parts
/// (the pauldrons of the armoured model) are modelled around the origin and only make sense once placed by their
/// joint. Heuristic (MEDIUM, from R_HERO01 part 0.0.0: bounding box centre 1.5 from the origin, 30 units from its joints):
/// a part is joint-local when its bounding box centre is within 4 units of the origin and more than 12 units from
/// the bind position of every joint its vertices use.
pub fn local_parts(mesh: &gow2_formats::skin::SkinMesh, skel: &Skeleton) -> Vec<bool> {
    let nparts = mesh.part.iter().copied().max().map_or(0, |m| m as usize + 1);
    let mut lo = vec![[f32::MAX; 3]; nparts];
    let mut hi = vec![[f32::MIN; 3]; nparts];
    let mut joints: Vec<std::collections::BTreeSet<usize>> = vec![Default::default(); nparts];
    for (i, v) in mesh.verts.iter().enumerate() {
        let p = mesh.part[i] as usize;
        for k in 0..3 {
            let x = v[k] as f32 / 16.0;
            lo[p][k] = lo[p][k].min(x);
            hi[p][k] = hi[p][k].max(x);
        }
        joints[p].insert(mesh.joints[i] as usize);
    }
    (0..nparts)
        .map(|p| {
            if lo[p][0] > hi[p][0] {
                return false;
            }
            let c = [(lo[p][0] + hi[p][0]) / 2.0, (lo[p][1] + hi[p][1]) / 2.0, (lo[p][2] + hi[p][2]) / 2.0];
            let from_origin = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            let nearest = joints[p]
                .iter()
                .filter_map(|&j| skel.bind_world.get(j))
                .map(|m| ((c[0] - m[12]).powi(2) + (c[1] - m[13]).powi(2) + (c[2] - m[14]).powi(2)).sqrt())
                .fold(f32::MAX, f32::min);
            from_origin < 4.0 && nearest > 12.0
        })
        .collect()
}

/// Skinning where joint-local vertices go through the joint world matrix and the others through the skin matrix.
pub fn skin_positions_mixed(
    bind_pos: &[[f32; 3]],
    joints: &[i64],
    local: &[bool],
    skin: &[Mat4],
    world: &[Mat4],
) -> Vec<[f32; 3]> {
    bind_pos
        .iter()
        .zip(joints)
        .zip(local)
        .map(|((p, &j), &is_local)| {
            let m = if is_local { world.get(j as usize) } else { skin.get(j as usize) };
            match m {
                Some(m) => transform_point(m, *p),
                None => *p,
            }
        })
        .collect()
}


/// Two-joint blend skinning: `(1 - t) * (p * M[joint_a]) + t * (p * M[joint_b])`, with `t` the vertex weight.
/// Joint-local vertices use the joint world matrices, the others the skin matrices.
#[allow(clippy::too_many_arguments)]
pub fn skin_positions_blend(
    bind_pos: &[[f32; 3]],
    joints_a: &[i64],
    joints_b: &[i64],
    weight: &[f32],
    local: &[bool],
    skin: &[Mat4],
    world: &[Mat4],
) -> Vec<[f32; 3]> {
    (0..bind_pos.len())
        .map(|i| {
            let set = if local[i] { world } else { skin };
            let p = bind_pos[i];
            let a = set.get(joints_a[i] as usize).map_or(p, |m| transform_point(m, p));
            let t = weight[i];
            if t <= 0.0 || joints_b[i] == joints_a[i] {
                return a;
            }
            let b = set.get(joints_b[i] as usize).map_or(p, |m| transform_point(m, p));
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
        })
        .collect()
}


/// Weighted blend of several local poses (weights should sum to 1): rotations are blended as normalised quaternions along the
/// shorter arc, translations and scales linearly. Used to cross-fade clips.
pub fn blend_poses(poses: &[(&[Trs], f32)]) -> Vec<Trs> {
    let n = poses.first().map_or(0, |p| p.0.len());
    (0..n)
        .map(|j| {
            let first = poses[0].0[j];
            let (mut t, mut s, mut q) = ([0.0f64; 3], [0.0f64; 3], [0.0f64; 4]);
            for (pose, w) in poses {
                let p = pose[j];
                let w = *w as f64;
                for k in 0..3 {
                    t[k] += p.t[k] * w;
                    s[k] += p.s[k] * w;
                }
                let dot: f64 = p.q.iter().zip(&first.q).map(|(a, b)| a * b).sum();
                let sign = if dot < 0.0 { -1.0 } else { 1.0 };
                for k in 0..4 {
                    q[k] += p.q[k] * w * sign;
                }
            }
            let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            if len > 0.0 {
                q.iter_mut().for_each(|x| *x /= len);
            } else {
                q = first.q;
            }
            Trs { t, q, s }
        })
        .collect()
}
