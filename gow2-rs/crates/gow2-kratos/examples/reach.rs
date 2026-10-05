//! Which parts of a level can Kratos walk to from the start? A flood fill over the controller's own floor and wall queries.
//!
//! `cargo run --release --example reach -- <level.WAD> <x> <y> <z> <out.png>`: writes a top-down map (reachable floor bright green, floor that is
//! not reachable dark red, other floor-less area black) and prints how much of the floor is reachable.

use std::collections::VecDeque;

use gow2_formats::{sheet, wad};
use gow2_kratos::{
    world::{sheet_world, World},
    BODY_HEIGHT, BODY_RADIUS, STEP_DOWN, STEP_UP,
};

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn write_png(path: &str, w: usize, h: usize, rgb: &[u8]) {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    // zlib with stored blocks
    let mut z = vec![0x78, 0x01];
    for (i, chunk) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |t: &[u8], d: &[u8]| {
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        let mut td = t.to_vec();
        td.extend_from_slice(d);
        out.extend_from_slice(&td);
        out.extend_from_slice(&crc32(&td).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let sh = sheet::find(&recs).expect("no collision sheet");
    // REACH_IGNORE=name1,name2 drops the walls of those surfaces (an experiment: which surface is the barrier?)
    let ignore: Vec<String> = std::env::var("REACH_IGNORE").map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
    let world = if ignore.is_empty() {
        sheet_world(&sh)
    } else {
        let mut keep = Vec::new();
        for p in &sh.polys {
            let s = sh.surface(p);
            if p.normal[1].abs() <= 0.5 && ignore.iter().any(|i| s.name.contains(i.as_str())) {
                continue;
            }
            let (class_skip, own) = if p.normal[1].abs() > 0.5 { (sheet::flag::SKIP_FLOOR, sheet::flag::HERO_OWN) } else { (sheet::flag::SKIP_WALK, sheet::flag::HERO_OWN) };
            if s.flags & (class_skip | own) == 0 {
                keep.extend(p.triangles());
            }
        }
        gow2_kratos::world::CollisionWorld::new(keep, 32.0)
    };
    let start: Vec<f32> = a[2..5].iter().map(|s| s.parse().unwrap()).collect();
    let step = 4.0f32;
    let ok = |x: f32, z: f32, from_y: f32| -> Option<f32> {
        let fy = world.floor(x, z, from_y + STEP_UP)?;
        if fy < from_y - STEP_DOWN {
            return None;
        }
        let y0 = fy + STEP_UP;
        let push = world.push_out([x, fy, z], BODY_RADIUS, y0, (fy + BODY_HEIGHT - BODY_RADIUS).max(y0));
        if push[0].abs() + push[1].abs() > 0.5 {
            return None;
        }
        Some(fy)
    };
    let (lo, hi) = {
        let (l, h) = world.bounds().unwrap();
        (l, h)
    };
    let (nx, nz) = (((hi[0] - lo[0]) / step) as usize + 2, ((hi[2] - lo[2]) / step) as usize + 2);
    let idx = |x: f32, z: f32| (((x - lo[0]) / step) as usize, ((z - lo[2]) / step) as usize);
    let mut seen = vec![f32::NAN; nx * nz];
    let mut q = VecDeque::new();
    let sy = world.floor(start[0], start[2], start[1] + STEP_UP).expect("no floor at the start");
    let (sx, sz) = idx(start[0], start[2]);
    seen[sz * nx + sx] = sy;
    q.push_back((start[0], start[2], sy));
    let mut reach = 0usize;
    while let Some((x, z, y)) = q.pop_front() {
        reach += 1;
        for (dx, dz) in [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step), (step, step), (-step, -step), (step, -step), (-step, step)] {
            let (px, pz) = (x + dx, z + dz);
            if px < lo[0] || pz < lo[2] || px > hi[0] || pz > hi[2] {
                continue;
            }
            let (ix, iz) = idx(px, pz);
            if !seen[iz * nx + ix].is_nan() {
                continue;
            }
            if let Some(fy) = ok(px, pz, y) {
                seen[iz * nx + ix] = fy;
                q.push_back((px, pz, fy));
            }
        }
    }
    // why the fill stops: from the reachable cell furthest along -z (or the one given as the sixth argument "x,z"), probe around it
    let mut best: Option<(f32, f32, f32)> = None;
    for iz in 0..nz {
        for ix in 0..nx {
            let y = seen[iz * nx + ix];
            if !y.is_nan() {
                let (x, z) = (lo[0] + ix as f32 * step, lo[2] + iz as f32 * step);
                if best.map_or(true, |b| z < b.2) {
                    best = Some((x, y, z));
                }
            }
        }
    }
    let probe_at = a.get(6).map(|s| s.split(',').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>());
    if let Some(b) = best {
        let (px, pz) = probe_at.as_ref().map_or((b.0, b.2), |p| (p[0], p[1]));
        let py = world.floor(px, pz, hi[1]).unwrap_or(b.1);
        println!("probing around ({px:.0}, {py:.1}, {pz:.0})");
        for k in 0..16 {
            let ang = k as f32 / 16.0 * std::f32::consts::TAU;
            let mut line = String::new();
            for d in [8.0f32, 16.0, 24.0, 40.0] {
                let (x, z) = (px + ang.cos() * d, pz + ang.sin() * d);
                let r = match world.floor(x, z, py + STEP_UP) {
                    None => "nofloor".to_string(),
                    Some(fy) if fy < py - STEP_DOWN => format!("drop{:.0}", py - fy),
                    Some(fy) => {
                        let y0 = fy + STEP_UP;
                        let p = world.push_out([x, fy, z], BODY_RADIUS, y0, (fy + BODY_HEIGHT - BODY_RADIUS).max(y0));
                        if p[0].abs() + p[1].abs() > 0.5 { format!("wall({:.0},{:.0})", p[0], p[1]) } else { format!("ok{:+.0}", fy - py) }
                    }
                };
                line.push_str(&format!("{r:<14}"));
            }
            println!("  angle {:>3.0}  {line}", ang.to_degrees());
        }
    }
    // the floor that exists but was not reached
    let mut rgb = vec![16u8; nx * nz * 3];
    let mut floor_cells = 0usize;
    for iz in 0..nz {
        for ix in 0..nx {
            let (x, z) = (lo[0] + ix as f32 * step, lo[2] + iz as f32 * step);
            let p = (iz * nx + ix) * 3;
            if !seen[iz * nx + ix].is_nan() {
                let t = ((seen[iz * nx + ix] - lo[1]) / (hi[1] - lo[1]).max(1.0)).clamp(0.0, 1.0);
                rgb[p..p + 3].copy_from_slice(&[40, (120.0 + 135.0 * t) as u8, 60]);
                floor_cells += 1;
            } else if world.floor(x, z, hi[1]).is_some() {
                rgb[p..p + 3].copy_from_slice(&[150, 30, 30]);
                floor_cells += 1;
            }
        }
    }
    write_png(&a[5], nx, nz, &rgb);
    println!("reachable cells {reach} of {floor_cells} floor cells ({:.0} % ), step {step}, map {nx} x {nz}, origin x {} z {}", 100.0 * reach as f32 / floor_cells.max(1) as f32, lo[0], lo[2]);
}

