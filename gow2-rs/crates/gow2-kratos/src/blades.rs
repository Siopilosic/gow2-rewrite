//! Where Kratos's blades are: on his back, in his hands, or flying on their chains (`docs/animation.md`, "Blade attachment").
//!
//! The clips animate a free joint (`lWeapOH` / `rWeapOH`) for each blade; the game snaps the blade to the back joint or the hand joint
//! when the free joint is within the snap distance of it (`FUN_00245f78`) and otherwise lets it trail the free joint (`FUN_0023e950`):
//!
//! * `d1 = |free - hand|`, `d0 = |free - back|`; if `d1 < d0` the mode is hand (1) when `d1 < r`, else free (2); otherwise back (0) when
//!   `d0 < r`, else free (2). `r` is the record's snap distance times 16 (0.35 m for the blades).
//! * the mode is kept while a forced mode is active (`Scr_ForceAttachment`) or while the current move has `MOV+4 & 2` (the stances).
//! * the blade's matrix follows the slot joint's world matrix: it snaps (weight 1) on the back and in the hand, and in the free mode it
//!   moves toward the free joint by `0.4 x dt x 60` of the gap per update (`GBL+0x90`).
//!
//! RAM check (`analysis/runtime/ingame2`, blades on the back): the blade object's matrix at `object + 0x20` is bit for bit the world
//! matrix of `leftBladeBack` / `rightBladeBack`, so the blade needs no offset from its slot joint (CONFIRMED).

/// A row-major 4x4 matrix in the game's row-vector convention: rows 0 to 2 are the axes, row 3 the translation.
pub type Mat = [f32; 16];

/// Follow weight in the free mode (`GBL + 0x90`).
pub const FREE_FOLLOW: f32 = 0.4;

/// Slot joints of one blade, as indices into the skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    /// Slot 0, stowed on the back (`LeftBladeBack`).
    pub back: usize,
    /// Slot 1, the hand (`LWeapIH`).
    pub hand: usize,
    /// Slot 2, the free joint the clips animate (`LWeapOH`).
    pub free: usize,
    /// The joint the chain starts from (`LChain`).
    pub chain: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Back = 0,
    Hand = 1,
    Free = 2,
}

fn pos(m: &Mat) -> [f32; 3] {
    [m[12], m[13], m[14]]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// How much wider than the record's snap distance the hand catches a blade. The rule as decoded uses the record's 0.35 m for both slots, but the clips put the left free
/// joint 0.35 to 0.4 m from the left hand during the stance (the right one 0.2 to 0.25 m), so with the exact distance the left blade hung beside the hand while the right one
/// sat in it. A wider catch for the hand keeps both in the hands (visual choice, LOW; the back keeps the record's distance).
pub const HAND_SNAP_SCALE: f32 = 1.6;

/// The mode the distance rule picks.
pub fn choose_mode(back: [f32; 3], hand: [f32; 3], free: [f32; 3], snap_units: f32) -> Mode {
    let (d1, d0) = (dist(free, hand), dist(free, back));
    if d1 < d0 {
        if d1 < snap_units * HAND_SNAP_SCALE { Mode::Hand } else { Mode::Free }
    } else if d0 < snap_units {
        Mode::Back
    } else {
        Mode::Free
    }
}

/// Unit quaternion (x, y, z, w) of the rotation in the rows of `m` (scale removed).
fn quat(m: &Mat) -> [f32; 4] {
    let norm = |r: [f32; 3]| {
        let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-12);
        [r[0] / l, r[1] / l, r[2] / l]
    };
    let (x, y, z) = (norm([m[0], m[1], m[2]]), norm([m[4], m[5], m[6]]), norm([m[8], m[9], m[10]]));
    // the matrix acts on row vectors, so the column-vector rotation is its transpose: R[i][j] = rows[j][i]
    let (r00, r01, r02) = (x[0], y[0], z[0]);
    let (r10, r11, r12) = (x[1], y[1], z[1]);
    let (r20, r21, r22) = (x[2], y[2], z[2]);
    let tr = r00 + r11 + r22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(r21 - r12) / s, (r02 - r20) / s, (r10 - r01) / s, 0.25 * s]
    } else if r00 > r11 && r00 > r22 {
        let s = (1.0 + r00 - r11 - r22).sqrt() * 2.0;
        [0.25 * s, (r01 + r10) / s, (r02 + r20) / s, (r21 - r12) / s]
    } else if r11 > r22 {
        let s = (1.0 + r11 - r00 - r22).sqrt() * 2.0;
        [(r01 + r10) / s, 0.25 * s, (r12 + r21) / s, (r02 - r20) / s]
    } else {
        let s = (1.0 + r22 - r00 - r11).sqrt() * 2.0;
        [(r02 + r20) / s, (r12 + r21) / s, 0.25 * s, (r10 - r01) / s]
    };
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-12);
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}

/// Row-major matrix from a unit quaternion and a position.
fn from_quat(q: [f32; 4], p: [f32; 3]) -> Mat {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    // column-vector rotation R; the stored rows are its columns
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [
        r[0][0], r[1][0], r[2][0], 0.0, //
        r[0][1], r[1][1], r[2][1], 0.0, //
        r[0][2], r[1][2], r[2][2], 0.0, //
        p[0], p[1], p[2], 1.0,
    ]
}

/// One blade: its slots, the snap distance, the current mode and matrix.
#[derive(Debug, Clone)]
pub struct Blade {
    pub slots: Slots,
    /// Snap distance in units (the record's metres times 16).
    pub snap: f32,
    pub mode: Mode,
    pub matrix: Mat,
    started: bool,
}

impl Blade {
    pub fn new(slots: Slots, snap_metres: f32) -> Self {
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        Blade { slots, snap: snap_metres * 16.0, mode: Mode::Back, matrix: m, started: false }
    }

    /// Updates the mode and the matrix from the skeleton's world matrices. `dt` in seconds; `keep_mode` holds the current mode (a stance
    /// or a forced attachment).
    pub fn update(&mut self, world: &[Mat], dt: f32, keep_mode: bool) {
        let s = self.slots;
        if !keep_mode || !self.started {
            self.mode = choose_mode(pos(&world[s.back]), pos(&world[s.hand]), pos(&world[s.free]), self.snap);
        }
        let target = &world[match self.mode {
            Mode::Back => s.back,
            Mode::Hand => s.hand,
            Mode::Free => s.free,
        }];
        let k = match self.mode {
            Mode::Free if self.started => (FREE_FOLLOW * dt * 60.0).min(1.0),
            _ => 1.0,
        };
        if k >= 1.0 {
            // a snap copies the joint's matrix as it is
            self.matrix = *target;
        } else {
            let (qa, qb) = (quat(&self.matrix), quat(target));
            let dot = qa.iter().zip(&qb).map(|(a, b)| a * b).sum::<f32>();
            let sign = if dot < 0.0 { -1.0 } else { 1.0 };
            let q = [0, 1, 2, 3].map(|i| qa[i] + (qb[i] * sign - qa[i]) * k);
            let l = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
            let (pa, pb) = (pos(&self.matrix), pos(target));
            self.matrix = from_quat([q[0] / l, q[1] / l, q[2] / l, q[3] / l], [0, 1, 2].map(|i| pa[i] + (pb[i] - pa[i]) * k));
        }
        self.started = true;
    }

    /// The point the chain is attached to: the blade's origin.
    pub fn origin(&self) -> [f32; 3] {
        pos(&self.matrix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: f32 = 0.35 * 16.0;

    #[test]
    fn the_distance_rule_picks_back_hand_or_free() {
        let (back, hand) = ([0.0, 0.0, 0.0], [100.0, 0.0, 0.0]);
        // the free joint 0.02 m (0.3 units) from the back joint: stowed (navIdle in RAM)
        assert_eq!(choose_mode(back, hand, [0.3, 0.0, 0.0], SNAP), Mode::Back);
        // next to the hand: held
        assert_eq!(choose_mode(back, hand, [98.0, 0.0, 0.0], SNAP), Mode::Hand);
        // 0.6 to 0.9 m from both (navCombatIdle): free, on the chain
        assert_eq!(choose_mode(back, hand, [50.0, 12.0, 0.0], SNAP), Mode::Free);
        // closer to the back but outside the snap distance: free
        assert_eq!(choose_mode(back, hand, [30.0, 0.0, 0.0], SNAP), Mode::Free);
    }

    fn at(p: [f32; 3]) -> Mat {
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m[12..15].copy_from_slice(&p);
        m
    }

    fn rig() -> (Blade, Vec<Mat>) {
        // joints: 0 back, 1 hand, 2 free, 3 chain
        let w = vec![at([0.0, 0.0, 0.0]), at([100.0, 0.0, 0.0]), at([0.0, 0.0, 0.0]), at([0.0, 50.0, 0.0])];
        (Blade::new(Slots { back: 0, hand: 1, free: 2, chain: 3 }, 0.35), w)
    }

    #[test]
    fn on_the_back_the_blade_has_exactly_the_joints_matrix() {
        let (mut b, mut w) = rig();
        let mut rotated = at([5.0, 6.0, 7.0]);
        rotated[0] = 0.0;
        rotated[1] = 1.0;
        rotated[4] = -1.0;
        rotated[5] = 0.0;
        w[0] = rotated;
        w[2] = rotated;
        b.update(&w, 1.0 / 60.0, false);
        assert_eq!(b.mode, Mode::Back);
        assert_eq!(b.matrix, rotated);
    }

    #[test]
    fn in_the_free_mode_the_blade_trails_the_free_joint_by_forty_percent_per_frame() {
        let (mut b, mut w) = rig();
        b.update(&w, 1.0 / 60.0, false);
        assert_eq!(b.mode, Mode::Back);
        // the free joint flies out to 60 units from both slots
        w[2] = at([50.0, 40.0, 0.0]);
        b.update(&w, 1.0 / 60.0, false);
        assert_eq!(b.mode, Mode::Free);
        let p = b.origin();
        assert!((p[0] - 20.0).abs() < 1e-3 && (p[1] - 16.0).abs() < 1e-3, "{p:?}");
        for _ in 0..30 {
            b.update(&w, 1.0 / 60.0, false);
        }
        assert!(dist(b.origin(), [50.0, 40.0, 0.0]) < 0.1);
    }

    #[test]
    fn a_kept_mode_does_not_switch() {
        let (mut b, mut w) = rig();
        b.update(&w, 1.0 / 60.0, false);
        w[2] = at([98.0, 0.0, 0.0]);
        b.update(&w, 1.0 / 60.0, true);
        assert_eq!(b.mode, Mode::Back, "kept while a stance or a forced mode holds it");
        b.update(&w, 1.0 / 60.0, false);
        assert_eq!(b.mode, Mode::Hand);
    }

    #[test]
    fn the_quaternion_round_trips_a_rotation() {
        let mut m = at([1.0, 2.0, 3.0]);
        // 90 degrees about y for row vectors
        m[0] = 0.0;
        m[2] = -1.0;
        m[8] = 1.0;
        m[10] = 0.0;
        let back = from_quat(quat(&m), [1.0, 2.0, 3.0]);
        for i in 0..16 {
            assert!((back[i] - m[i]).abs() < 1e-5, "{i}: {} vs {}", back[i], m[i]);
        }
    }
}

