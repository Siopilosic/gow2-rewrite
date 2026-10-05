//! Lists the effect groups of a WAD: the go node, model, animation, emitters (subtype, joint, channel, rate, shape and its render routine and blend) and rate tracks.
//! `cargo run --release -p gow2-fx --example fx_groups -- ../extracted/pak/R_M_EARTH0.WAD`
use gow2_fx::bank::Bank;

fn main() {
    let path = std::env::args().nth(1).expect("WAD");
    let data = std::fs::read(&path).unwrap();
    let bank = Bank::load(&data);
    println!("{} shapes, {} emitters, {} geometries, {} effect groups", bank.shapes.len(), bank.emitters.len(), bank.geoms.len(), bank.effects.len());
    for (i, e) in bank.effects.iter().enumerate() {
        println!("effect {i}: go {:?} model {:?} anm {:?} duration {:.2} s, rig {} bytes, tracks {:?}", e.go, e.model, e.anm, e.duration, e.rig.len(), e.tracks.iter().map(|(k, t)| (*k, t.keys.len())).collect::<Vec<_>>());
        for n in &e.emitters {
            let Some(em) = bank.emitters.get(n) else {
                println!("    {n}: not an emitter record (field or geometry)");
                continue;
            };
            let sh = bank.shape_index.get(&em.shape_name).map(|&k| &bank.shapes[k]);
            println!(
                "    {n:18} sub {:2} joint {:3} chan {:5} rate {:6.1} speed {:5.2}+-{:4.2} spread {:.2} P0 {:.1?} -> {} {}",
                em.subtype,
                em.joint,
                em.channel,
                em.rate(),
                em.speed(),
                em.speed_range(),
                em.spread(),
                [em.p[0], em.p[1], em.p[2]],
                em.shape_name,
                sh.map_or("(no shape)".to_string(), |s| format!("[{} life {:.2} render {} {:?} flags {:#x} material {:?}]", s.name, s.life, s.render, s.blend(), s.flags, s.material))
            );
            if let Some(t) = e.tracks.get(&em.channel) {
                println!("        rate track: dt {:.4} keys {:?}", t.dt, t.keys.iter().take(8).collect::<Vec<_>>());
            }
        }
    }
}
