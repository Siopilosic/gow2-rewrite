//! Runs an effect without a renderer and prints, every 0.1 s, how many sprites are alive per shape, their size range, mean alpha and the spread around the origin.
//! `cargo run --release -p gow2-fx --example fx_run -- ../extracted/pak/R_M_EARTH0.WAD goearthstomp [scale]`
use gow2_fx::bank::Bank;
use gow2_fx::play::Player;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let scale: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let mut p = Player::new(Bank::load(&data));
    p.attach_rigs(&data);
    let root = [scale, 0.0, 0.0, 0.0, 0.0, scale, 0.0, 0.0, 0.0, 0.0, scale, 0.0, 0.0, 0.0, 0.0, 1.0];
    assert!(p.start(&a[2], root, None), "no such effect");
    if let Some(def) = p.bank.effect(&a[2]) {
        for tt in [0.0f32, 0.3] {
            if let Some(j) = p.joints(def, None, tt) {
                for (i, m) in j.iter().enumerate() {
                    let l = |o: usize| (m[o] * m[o] + m[o + 1] * m[o + 1] + m[o + 2] * m[o + 2]).sqrt();
                    println!("joint {i} at {tt}: scale {:.2} {:.2} {:.2} translation {:.1} {:.1} {:.1}", l(0), l(4), l(8), m[12], m[13], m[14]);
                }
            }
        }
    }
    let mut t = 0.0f32;
    for step in 0..60 {
        p.update(1.0 / 30.0);
        t += 1.0 / 30.0;
        if step % 3 != 2 {
            continue;
        }
        let sp = p.sprites();
        let mut per: std::collections::BTreeMap<usize, (usize, f32, f32, f32, f32)> = std::collections::BTreeMap::new();
        for s in &sp {
            let e = per.entry(s.shape).or_insert((0, f32::MAX, 0.0, 0.0, 0.0));
            e.0 += 1;
            e.1 = e.1.min(s.size);
            e.2 = e.2.max(s.size);
            e.3 += s.rgba[3];
            e.4 = e.4.max((s.pos[0] * s.pos[0] + s.pos[1] * s.pos[1] + s.pos[2] * s.pos[2]).sqrt());
        }
        let line: Vec<String> = per.iter().map(|(k, (n, lo, hi, al, r))| format!("{}: {n} size {lo:.1}..{hi:.1} alpha {:.2} reach {r:.0}", p.shape(*k).name.trim_start_matches("PTC_"), al / *n as f32)).collect();
        println!("t {t:.2} instances {} live {}  {}", p.instances.len(), sp.len(), line.join(" | "));
    }
}
