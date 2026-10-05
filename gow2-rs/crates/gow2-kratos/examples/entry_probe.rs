//! Which move do the entry branches start for a given input? `cargo run --example entry_probe -- <hero WAD> <right x> <right y> [left x] [left y]`

use std::sync::Arc;

use gow2_formats::{dc::Dc, wad};
use gow2_kratos::moves::{Env, Input, MoveSys, Pad, Sticks, STATE_GROUND};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let ms = Arc::new(Dc::from_records(&recs).expect("no DC data").move_set());
    let f = |i: usize| a.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
    let mut sys = MoveSys::new(ms);
    let env = Env { state_mask: STATE_GROUND, health: 200.0, ..Default::default() };
    let inp = Input { pad: Pad { cur: 0, prev: 0 }, sticks: Sticks { left: [f(4), f(5)], right: [f(2), f(3)] } };
    for b in sys.set.entry.iter().filter(|b| b.name.contains("Evade")) {
        println!("{}: target {:?} input_ok {} state_ok {:?}", b.name, b.target, gow2_kratos::moves::input_ok(b, &inp), gow2_kratos::moves::state_ok(b, None, None, &env));
    }
    let t = sys.update(1.0 / 60.0, &inp, &env, &|_| Some(1.0));
    println!("started {:?}: {:?}", t.started, t.started.map(|m| sys.set.moves[m].name.clone()));
}

