//! What the controller needs to know about the level: floors, ceilings and walls.
//!
//! `CollisionWorld` answers from the level's real collision polygons (the `RIB_sheet` record, `docs/collision.md` section 3),
//! which the front end passes in as oriented triangles. `TriWorld` is the older floor-only stand-in built from render triangles;
//! it is kept for levels without a sheet.

use std::collections::HashMap;

/// A level as the controller sees it.
pub trait World {
    /// Height of the highest floor at (`x`, `z`) that is at or below `max_y`, if there is one.
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32>;

    /// Height of the lowest downward-facing surface at (`x`, `z`) that is at or above `min_y`, if there is one.
    fn ceiling(&self, _x: f32, _z: f32, _min_y: f32) -> Option<f32> {
        None
    }

    /// Fraction (0 to 1) along the segment `a` to `b` at which it first meets a solid surface, if it does (a camera or line-of-sight test).
    fn raycast(&self, _a: [f32; 3], _b: [f32; 3]) -> Option<f32> {
        None
    }

    /// Horizontal displacement (x, z) that moves a body of `radius` out of the walls around `pos`. The body is sampled with
    /// spheres whose centres run from `y0` to `y1`; wall contacts below `y0` are ignored (steps and kerbs).
    fn push_out(&self, _pos: [f32; 3], _radius: f32, _y0: f32, _y1: f32) -> [f32; 2] {
        [0.0, 0.0]
    }
}

/// An infinite flat floor at a fixed height.
#[derive(Debug, Clone, Copy)]
pub struct FlatGround(pub f32);

impl World for FlatGround {
    fn floor(&self, _x: f32, _z: f32, max_y: f32) -> Option<f32> {
        (self.0 <= max_y).then_some(self.0)
    }
}

/// Floor triangles in a uniform grid over the XZ plane.
#[derive(Debug, Clone, Default)]
pub struct TriWorld {
    tris: Vec<[[f32; 3]; 3]>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    cell: f32,
}

/// Triangles steeper than this (normal y below it) are walls, not floors.
pub const MIN_FLOOR_NORMAL_Y: f32 = 0.5;

impl TriWorld {
    /// Builds the index from world-space triangles, keeping only those that face upward enough to stand on.
    pub fn new(all: impl IntoIterator<Item = [[f32; 3]; 3]>, cell: f32) -> Self {
        let mut w = TriWorld { tris: Vec::new(), grid: HashMap::new(), cell };
        for t in all {
            let (a, b, c) = (t[0], t[1], t[2]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            // the winding of the original strips is not known to face outward, so accept either side
            if len < 1e-6 || (n[1] / len).abs() < MIN_FLOOR_NORMAL_Y {
                continue;
            }
            let id = w.tris.len() as u32;
            w.tris.push(t);
            let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (z0, z1) = (a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]));
            for cx in (x0 / cell).floor() as i32..=(x1 / cell).floor() as i32 {
                for cz in (z0 / cell).floor() as i32..=(z1 / cell).floor() as i32 {
                    w.grid.entry((cx, cz)).or_default().push(id);
                }
            }
        }
        w
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Bounding box (min, max) of the floor triangles.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.tris.iter().flatten();
        let first = *it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some((lo, hi))
    }
}

impl World for TriWorld {
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let ids = self.grid.get(&((x / self.cell).floor() as i32, (z / self.cell).floor() as i32))?;
        let mut best: Option<f32> = None;
        for &id in ids {
            let [a, b, c] = self.tris[id as usize];
            // barycentric coordinates in the XZ plane
            let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
            if d.abs() < 1e-9 {
                continue;
            }
            let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
            let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
            let l3 = 1.0 - l1 - l2;
            let eps = -1e-4;
            if l1 < eps || l2 < eps || l3 < eps {
                continue;
            }
            let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
            if y <= max_y && best.map_or(true, |b| y > b) {
                best = Some(y);
            }
        }
        best
    }
}

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The point of triangle `abc` closest to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [b[0] + w * (c[0] - b[0]), b[1] + w * (c[1] - b[1]), b[2] + w * (c[2] - b[2])];
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
}

/// Collision polygons (as triangles) in a uniform XZ grid, sorted into floors, ceilings and walls by their normal.
///
/// The triangles must be wound so that `(b-a) x (c-a)` points out of the solid (the sheet reader's polygons are).
#[derive(Debug, Clone, Default)]
pub struct CollisionWorld {
    tris: Vec<[V3; 3]>,
    kind: Vec<Kind>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    cell: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Floor,
    Ceiling,
    Wall,
}

impl CollisionWorld {
    pub fn new(all: impl IntoIterator<Item = [V3; 3]>, cell: f32) -> Self {
        let mut w = CollisionWorld { tris: Vec::new(), kind: Vec::new(), grid: HashMap::new(), cell };
        for t in all {
            let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let len = dot(n, n).sqrt();
            if len < 1e-6 {
                continue;
            }
            let ny = n[1] / len;
            let kind = if ny > MIN_FLOOR_NORMAL_Y {
                Kind::Floor
            } else if ny < -MIN_FLOOR_NORMAL_Y {
                Kind::Ceiling
            } else {
                Kind::Wall
            };
            let id = w.tris.len() as u32;
            w.tris.push(t);
            w.kind.push(kind);
            let x0 = t[0][0].min(t[1][0]).min(t[2][0]);
            let x1 = t[0][0].max(t[1][0]).max(t[2][0]);
            let z0 = t[0][2].min(t[1][2]).min(t[2][2]);
            let z1 = t[0][2].max(t[1][2]).max(t[2][2]);
            for cx in (x0 / cell).floor() as i32..=(x1 / cell).floor() as i32 {
                for cz in (z0 / cell).floor() as i32..=(z1 / cell).floor() as i32 {
                    w.grid.entry((cx, cz)).or_default().push(id);
                }
            }
        }
        w
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// (floors, ceilings, walls).
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |k| self.kind.iter().filter(|&&x| x == k).count();
        (n(Kind::Floor), n(Kind::Ceiling), n(Kind::Wall))
    }

    /// Bounding box (min, max) of all polygons.
    pub fn bounds(&self) -> Option<(V3, V3)> {
        let mut it = self.tris.iter().flatten();
        let first = *it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some((lo, hi))
    }

    /// Height of the plane of triangle `id` at (x, z) when (x, z) lies inside it in the XZ projection.
    fn height_at(&self, id: u32, x: f32, z: f32) -> Option<f32> {
        let [a, b, c] = self.tris[id as usize];
        let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
        if d.abs() < 1e-9 {
            return None;
        }
        let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
        let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
        let l3 = 1.0 - l1 - l2;
        let eps = -1e-4;
        (l1 >= eps && l2 >= eps && l3 >= eps).then(|| l1 * a[1] + l2 * b[1] + l3 * c[1])
    }

    fn cell_ids(&self, x: f32, z: f32) -> Option<&Vec<u32>> {
        self.grid.get(&((x / self.cell).floor() as i32, (z / self.cell).floor() as i32))
    }
}

impl World for CollisionWorld {
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for &id in self.cell_ids(x, z)? {
            if self.kind[id as usize] != Kind::Floor {
                continue;
            }
            if let Some(y) = self.height_at(id, x, z) {
                if y <= max_y && best.map_or(true, |b| y > b) {
                    best = Some(y);
                }
            }
        }
        best
    }

    fn ceiling(&self, x: f32, z: f32, min_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for &id in self.cell_ids(x, z)? {
            if self.kind[id as usize] != Kind::Ceiling {
                continue;
            }
            if let Some(y) = self.height_at(id, x, z) {
                if y >= min_y && best.map_or(true, |b| y < b) {
                    best = Some(y);
                }
            }
        }
        best
    }

    fn raycast(&self, a: [f32; 3], b: [f32; 3]) -> Option<f32> {
        // every triangle in the cells under the segment's bounding box, tested on both sides
        let (cx0, cx1) = ((a[0].min(b[0]) / self.cell).floor() as i32, (a[0].max(b[0]) / self.cell).floor() as i32);
        let (cz0, cz1) = ((a[2].min(b[2]) / self.cell).floor() as i32, (a[2].max(b[2]) / self.cell).floor() as i32);
        if (cx1 - cx0 + 1) as i64 * (cz1 - cz0 + 1) as i64 > 4096 {
            return None;
        }
        let d = sub(b, a);
        let mut best: Option<f32> = None;
        for cx in cx0..=cx1 {
            for cz in cz0..=cz1 {
                let Some(ids) = self.grid.get(&(cx, cz)) else { continue };
                for &id in ids {
                    let [p0, p1, p2] = self.tris[id as usize];
                    // Moller-Trumbore
                    let (e1, e2) = (sub(p1, p0), sub(p2, p0));
                    let h = cross(d, e2);
                    let det = dot(e1, h);
                    if det.abs() < 1e-9 {
                        continue;
                    }
                    let inv = 1.0 / det;
                    let s = sub(a, p0);
                    let u = dot(s, h) * inv;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = cross(s, e1);
                    let v = dot(d, q) * inv;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let t = dot(e2, q) * inv;
                    if (0.0..=1.0).contains(&t) && best.map_or(true, |bt| t < bt) {
                        best = Some(t);
                    }
                }
            }
        }
        best
    }

    fn push_out(&self, pos: V3, radius: f32, y0: f32, y1: f32) -> [f32; 2] {
        // gather the walls in the cells the body touches
        let (cx0, cx1) = (((pos[0] - radius) / self.cell).floor() as i32, ((pos[0] + radius) / self.cell).floor() as i32);
        let (cz0, cz1) = (((pos[2] - radius) / self.cell).floor() as i32, ((pos[2] + radius) / self.cell).floor() as i32);
        let mut walls: Vec<u32> = Vec::new();
        for cx in cx0..=cx1 {
            for cz in cz0..=cz1 {
                if let Some(ids) = self.grid.get(&(cx, cz)) {
                    walls.extend(ids.iter().copied().filter(|&i| self.kind[i as usize] == Kind::Wall));
                }
            }
        }
        if walls.is_empty() {
            return [0.0, 0.0];
        }
        walls.sort_unstable();
        walls.dedup();
        let samples = (((y1 - y0) / radius).ceil().max(0.0) as usize) + 1;
        let mut total = [0.0f32, 0.0];
        // up to three passes, as the game's move loop does (docs/collision.md section 2)
        for _ in 0..3 {
            let mut moved = false;
            for s in 0..samples {
                let cy = if samples == 1 { y0 } else { y0 + (y1 - y0) * s as f32 / (samples - 1) as f32 };
                let centre = [pos[0] + total[0], cy, pos[2] + total[1]];
                for &id in &walls {
                    let [a, b, c] = self.tris[id as usize];
                    let q = closest_on_triangle(centre, a, b, c);
                    if q[1] < y0 - 1e-3 {
                        continue;
                    }
                    let (dx, dy, dz) = (centre[0] - q[0], centre[1] - q[1], centre[2] - q[2]);
                    let dh = (dx * dx + dz * dz).sqrt();
                    if dh < 1e-4 || dh * dh + dy * dy >= radius * radius {
                        continue;
                    }
                    // slide along the horizontal direction until the sphere just touches the point
                    let t = (radius * radius - dy * dy).sqrt() - dh;
                    if t > 1e-4 {
                        total[0] += dx / dh * t;
                        total[1] += dz / dh * t;
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        total
    }
}

/// A connected part of a level's floor that the controller can walk across.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// A floor point inside the region, near its middle (x, y, z).
    pub centre: [f32; 3],
    /// Number of grid cells (`step` units apart) it covers.
    pub cells: usize,
}

/// Splits the walkable floor of `world` into connected regions with a flood fill over the controller's own queries (floor within the step height,
/// no wall within the body radius). Largest first. Levels whose parts the game joins by script (doors, breakable walls) come out as several regions.
pub fn walkable_regions(world: &CollisionWorld, step: f32) -> Vec<Region> {
    use crate::{BODY_HEIGHT, BODY_RADIUS, STEP_DOWN, STEP_UP};
    use std::collections::HashSet;
    let Some((lo, hi)) = world.bounds() else { return Vec::new() };
    let (nx, nz) = (((hi[0] - lo[0]) / step) as usize + 2, ((hi[2] - lo[2]) / step) as usize + 2);
    let idx = |x: f32, z: f32| (((x - lo[0]) / step) as i32, ((z - lo[2]) / step) as i32);
    // a cell is a column and a floor level, so floors above each other are separate cells
    let key = |x: f32, z: f32, y: f32| {
        let (ix, iz) = idx(x, z);
        (ix, iz, (y / STEP_UP).round() as i32)
    };
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let free_at = |x: f32, z: f32, floor_y: f32| -> bool {
        let y0 = floor_y + STEP_UP;
        let p = world.push_out([x, floor_y, z], BODY_RADIUS, y0, (floor_y + BODY_HEIGHT - BODY_RADIUS).max(y0));
        p[0].abs() + p[1].abs() <= 0.5
    };
    let mut regions = Vec::new();
    for iz in 0..nz {
        for ix in 0..nx {
            let (x, z) = (lo[0] + ix as f32 * step, lo[2] + iz as f32 * step);
            // every floor in this column that is not yet in a region
            let mut y = hi[1] + 1.0;
            while let Some(fy) = world.floor(x, z, y) {
                y = fy - STEP_UP - 0.5;
                if seen.contains(&key(x, z, fy)) || !free_at(x, z, fy) {
                    continue;
                }
                seen.insert(key(x, z, fy));
                let mut cells = vec![[x, fy, z]];
                let mut q = std::collections::VecDeque::from([[x, fy, z]]);
                while let Some([cx, cy, cz]) = q.pop_front() {
                    for (dx, dz) in [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step)] {
                        let (px, pz) = (cx + dx, cz + dz);
                        if px < lo[0] || pz < lo[2] || px > hi[0] || pz > hi[2] {
                            continue;
                        }
                        let Some(f2) = world.floor(px, pz, cy + STEP_UP) else { continue };
                        if f2 < cy - STEP_DOWN || seen.contains(&key(px, pz, f2)) || !free_at(px, pz, f2) {
                            continue;
                        }
                        seen.insert(key(px, pz, f2));
                        cells.push([px, f2, pz]);
                        q.push_back([px, f2, pz]);
                    }
                }
                if cells.len() >= 25 {
                    let n = cells.len() as f32;
                    let mean = [cells.iter().map(|c| c[0]).sum::<f32>() / n, cells.iter().map(|c| c[2]).sum::<f32>() / n];
                    let d2 = |c: &[f32; 3]| (c[0] - mean[0]).powi(2) + (c[2] - mean[1]).powi(2);
                    let best = cells.iter().min_by(|a, b| d2(a).total_cmp(&d2(b))).unwrap();
                    regions.push(Region { centre: *best, cells: cells.len() });
                }
            }
        }
    }
    regions.sort_by(|a, b| b.cells.cmp(&a.cells));
    regions
}
/// How a collision polygon counts for the hero's movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Floor,
    Ceiling,
    Wall,
    /// In the sheet, but the hero's queries skip it (guides, water, `NoPlayerCollision`, ...).
    Skipped,
}

/// The sheet's polygons as oriented triangles with their class: floors and ceilings by normal (`|ny| > 0.5`), walls otherwise, and a polygon the hero's
/// movement queries skip (`SKIP_FLOOR` for floors and ceilings, `SKIP_WALK` for walls, plus his own `NoPlayerCollision`) as `Skipped`.
pub fn sheet_triangles(s: &gow2_formats::sheet::Sheet) -> Vec<([[f32; 3]; 3], Class)> {
    use gow2_formats::sheet::flag;
    let mut out = Vec::new();
    for p in &s.polys {
        let f = s.surface(p).flags;
        let (class, skip) = if p.normal[1] > 0.5 {
            (Class::Floor, flag::SKIP_FLOOR)
        } else if p.normal[1] < -0.5 {
            (Class::Ceiling, flag::SKIP_FLOOR)
        } else {
            (Class::Wall, flag::SKIP_WALK)
        };
        let solid = f & (skip | flag::HERO_OWN) == 0;
        for t in p.triangles() {
            out.push((t, if solid { class } else { Class::Skipped }));
        }
    }
    out
}

/// The controller's collision world for a level sheet.
pub fn sheet_world(s: &gow2_formats::sheet::Sheet) -> CollisionWorld {
    CollisionWorld::new(sheet_triangles(s).into_iter().filter(|(_, c)| *c != Class::Skipped).map(|(t, _)| t), 32.0)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn quad(y: f32, x0: f32, x1: f32, z0: f32, z1: f32) -> Vec<[[f32; 3]; 3]> {
        vec![[[x0, y, z0], [x1, y, z0], [x1, y, z1]], [[x0, y, z0], [x1, y, z1], [x0, y, z1]]]
    }

    #[test]
    fn floor_height_is_found_under_a_point() {
        let w = TriWorld::new(quad(5.0, -10.0, 10.0, -10.0, 10.0), 8.0);
        assert_eq!(w.floor(1.0, 2.0, 100.0), Some(5.0));
        assert_eq!(w.floor(50.0, 2.0, 100.0), None);
    }

    #[test]
    fn only_floors_at_or_below_the_limit_count_and_the_highest_wins() {
        let mut tris = quad(0.0, -10.0, 10.0, -10.0, 10.0);
        tris.extend(quad(20.0, -10.0, 10.0, -10.0, 10.0));
        let w = TriWorld::new(tris, 8.0);
        assert_eq!(w.floor(0.0, 0.0, 100.0), Some(20.0));
        assert_eq!(w.floor(0.0, 0.0, 10.0), Some(0.0));
        assert_eq!(w.floor(0.0, 0.0, -1.0), None);
    }

    #[test]
    fn a_slope_interpolates_the_height() {
        let tris = vec![[[0.0, 0.0, 0.0], [10.0, 5.0, 0.0], [0.0, 0.0, 10.0]]];
        let w = TriWorld::new(tris, 8.0);
        let y = w.floor(5.0, 1.0, 100.0).unwrap();
        assert!((y - 2.5).abs() < 1e-4, "{y}");
    }

    #[test]
    fn vertical_walls_are_not_floors() {
        let w = TriWorld::new(vec![[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]], 8.0);
        assert_eq!(w.triangle_count(), 0);
    }

    /// A wall in the plane x = `x` from z0 to z1 and y0 to y1.
    fn wall_x(x: f32, z0: f32, z1: f32, y0: f32, y1: f32) -> Vec<[V3; 3]> {
        vec![[[x, y0, z0], [x, y1, z0], [x, y1, z1]], [[x, y0, z0], [x, y1, z1], [x, y0, z1]]]
    }

    /// Upward-facing floor and downward-facing ceiling (the winding decides which).
    fn flat(y: f32, up: bool, half: f32) -> Vec<[V3; 3]> {
        let (a, b, c, d) = ([-half, y, -half], [half, y, -half], [half, y, half], [-half, y, half]);
        if up { vec![[a, c, b], [a, d, c]] } else { vec![[a, b, c], [a, c, d]] }
    }

    #[test]
    fn polygons_are_sorted_by_their_normal() {
        let mut t = flat(0.0, true, 50.0);
        t.extend(flat(40.0, false, 50.0));
        t.extend(wall_x(20.0, -50.0, 50.0, 0.0, 40.0));
        let w = CollisionWorld::new(t, 16.0);
        assert_eq!(w.counts(), (2, 2, 2));
        assert_eq!(w.floor(0.0, 0.0, 100.0), Some(0.0));
        assert_eq!(w.ceiling(0.0, 0.0, 5.0), Some(40.0));
        assert_eq!(w.ceiling(0.0, 0.0, 41.0), None, "nothing above the ceiling");
    }

    #[test]
    fn a_wall_pushes_the_body_back_to_its_radius() {
        let w = CollisionWorld::new(wall_x(20.0, -50.0, 50.0, 0.0, 40.0), 16.0);
        // 5 units from the wall with radius 9.6: pushed back 4.6 along -x
        let d = w.push_out([15.0, 0.0, 0.0], 9.6, 10.0, 25.0);
        assert!((d[0] + 4.6).abs() < 0.2, "push {d:?}");
        assert!(d[1].abs() < 0.2);
        // far enough away: no push
        assert_eq!(w.push_out([0.0, 0.0, 0.0], 9.6, 10.0, 25.0), [0.0, 0.0]);
        // from the other side the push goes the other way
        let d = w.push_out([25.0, 0.0, 0.0], 9.6, 10.0, 25.0);
        assert!(d[0] > 4.0, "push {d:?}");
    }

    #[test]
    fn a_low_kerb_is_not_a_wall() {
        // an 8-unit riser is below the step height y0 = 10, so it does not push
        let w = CollisionWorld::new(wall_x(20.0, -50.0, 50.0, 0.0, 8.0), 16.0);
        assert_eq!(w.push_out([15.0, 0.0, 0.0], 9.6, 10.0, 25.0), [0.0, 0.0]);
        // a tall one does
        let w = CollisionWorld::new(wall_x(20.0, -50.0, 50.0, 0.0, 40.0), 16.0);
        assert!(w.push_out([15.0, 0.0, 0.0], 9.6, 10.0, 25.0)[0] < -3.0);
    }

    #[test]
    fn a_ray_stops_at_the_first_surface_it_meets() {
        let w = CollisionWorld::new(wall_x(20.0, -50.0, 50.0, 0.0, 40.0), 16.0);
        let t = w.raycast([0.0, 10.0, 0.0], [40.0, 10.0, 0.0]).expect("hits the wall");
        assert!((t - 0.5).abs() < 1e-4, "{t}");
        assert!(w.raycast([0.0, 10.0, 0.0], [10.0, 10.0, 0.0]).is_none(), "stops short of it");
        assert!(w.raycast([0.0, 60.0, 0.0], [40.0, 60.0, 0.0]).is_none(), "passes above it");
    }

    #[test]
    fn a_corner_pushes_along_both_walls() {
        let mut t = wall_x(20.0, -50.0, 50.0, 0.0, 40.0);
        // a wall in the plane z = 20
        t.extend([[[-50.0, 0.0, 20.0], [-50.0, 40.0, 20.0], [50.0, 40.0, 20.0]], [[-50.0, 0.0, 20.0], [50.0, 40.0, 20.0], [50.0, 0.0, 20.0]]]);
        let w = CollisionWorld::new(t, 16.0);
        let d = w.push_out([16.0, 0.0, 16.0], 9.6, 10.0, 25.0);
        assert!(d[0] < -4.0 && d[1] < -4.0, "push {d:?}");
    }
}



