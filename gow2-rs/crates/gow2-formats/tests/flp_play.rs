//! The HUD movie runs: the engine's per-tick `SimKeyEvent` call moves the meter clips to the frames the game's variables ask for.

use std::path::PathBuf;

use gow2_formats::{
    flp::Flp,
    flp_play::{Player, Val},
    wad,
};

fn player() -> Option<Player> {
    let data = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD")).ok()?;
    let m = wad::records(&data).find(|r| r.name == "FLP_HUDA" && !r.data.is_empty())?;
    let mut p = Player::new(Flp::parse(m.data)?);
    p.clear_events();
    Some(p)
}

fn run(p: &mut Player, ticks: u32) {
    for _ in 0..ticks {
        p.call_root("SimKeyEvent");
        p.tick(1.0 / 30.0);
    }
}

#[test]
fn the_meters_follow_the_game_variables() {
    let Some(mut p) = player() else { return };
    p.set_num("PS2_HealthMeter_Level", 0.0);
    p.set_num("PS2_MagicMeter_Level", 0.0);
    p.set_num("PS2_HealthMeter_Value", 100.0);
    p.set_num("PS2_MagicMeter_Value", 50.0);
    p.set_num("PS2_MeterBar_Event", 1.0);
    p.set_num("PS2_HealthMeter_Event", 2.0);
    p.set_num("PS2_MagicMeter_Event", 2.0);
    run(&mut p, 120);
    // frame = 1 + value / 2 as a 1-based number: index 50 for 100 health, 25 for 50 magic
    let (hf, _) = p.instance_frame("MainMeterT/MainMeter/HealthMeter").expect("health fill");
    let (mf, _) = p.instance_frame("MainMeterT/MainMeter/MagicMeter").expect("magic fill");
    assert_eq!(hf, 50, "health frame");
    assert_eq!(mf, 25, "magic frame");
    let (bf, _) = p.instance_frame("MainMeterT/MainMeter/HealthMeterBlackBar").expect("health well");
    assert_eq!(bf, 51, "level 0 well length");
    println!("{}", p.dump(0));
    println!("root layer 17 key at frame 1: {:?}", p.flp.root.key_at(17, 1));
    let f = p.draw();
    assert!(f.shapes.len() > 5, "{} shapes drawn", f.shapes.len());
}
