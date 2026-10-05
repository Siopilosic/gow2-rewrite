use gow2_fx::bank::{Bank, GeomKind};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let b = Bank::load(&std::fs::read(&a[1]).unwrap());
    for (n, g) in &b.geoms {
        println!("geometry {n}: matrix {:?}", g.matrix.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
        if let GeomKind::Curve { segs, knots } = &g.kind {
            println!("   curve {} segments, knots {:?}", knots.len(), knots);
            // sample the curve at 8 parameters
            let n = knots.len();
            for k in 0..=8 {
                let t = knots[n - 1] * k as f32 / 8.0 * 0.99999;
                let mut i = 0;
                if n > 1 && knots[0] < t { i = 1; while i < n - 1 && knots[i] < t { i += 1; } }
                let ev = |w: [f32; 4]| { let mut o = [0.0f32; 4]; for c in 0..4 { o[c] = w[0] * segs[i * 16 + c] + w[1] * segs[i * 16 + 4 + c] + w[2] * segs[i * 16 + 8 + c] + w[3] * segs[i * 16 + 12 + c]; } o };
                let q = ev([t * t * t, t * t, t, 1.0]);
                print!("   ({:.1}, {:.1}, {:.1})", q[0] / q[3], q[1] / q[3], q[2] / q[3]);
            }
            println!();
        }
    }
    for (i, e) in b.effects.iter().enumerate() {
        for n in &e.emitters { if let Some(em) = b.emitters.get(n) { println!("effect {i} {:?} emitter {n} joint {} matrix t {:?} geom {:?}", e.go, em.joint, &em.matrix[12..15], em.geom); } }
    }
}
