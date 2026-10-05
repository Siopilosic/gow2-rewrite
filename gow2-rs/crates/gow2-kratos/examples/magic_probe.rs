//! Which entry branches does the magic button start for each selected magic? `cargo run --example magic_probe -- <hero WAD>`

use std::sync::Arc;

use gow2_formats::{dc::Dc, wad};
use gow2_kratos::moves::{self, Env, Input, MoveSys, Pad, Progress, Sticks, STATE_GROUND};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let ms = Arc::new(Dc::from_records(&recs).expect("no DC data").move_set());
    for id in [0u8, 1, 2, 3, 6, 16] {
        for (press_name, pad) in [("tap", Pad { cur: moves::pad::MAGIC, prev: 0 }), ("hold", Pad { cur: moves::pad::MAGIC, prev: moves::pad::MAGIC })] {
            let mut sys = MoveSys::new(ms.clone());
            let env = Env { state_mask: STATE_GROUND, health: 200.0, progress: Progress { selected_magic: id, magic_ok: true, ..Progress::default() }, ..Default::default() };
            let inp = Input { pad, sticks: Sticks::default() };
            // some idle ticks first, as in the game
            for _ in 0..30 {
                sys.update(1.0 / 60.0, &Input::default(), &env, &|_| Some(1.0));
            }
            let t = sys.update(1.0 / 60.0, &inp, &env, &|_| Some(1.0));
            println!("magic {id:2} {press_name}: started {:?}", t.started.map(|m| sys.set.moves[m].name.clone()));
            if id == 1 && press_name == "tap" {
                for b in sys.set.entry.iter().filter(|b| b.button == 6) {
                    println!("   {} input_ok {} state_ok {:?}", b.name, moves::input_ok(b, &inp), moves::state_ok(b, None, None, &env));
                }
            }
        }
    }
}
