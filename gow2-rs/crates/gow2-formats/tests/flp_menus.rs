//! The menus of the HUD movie answer the engine's event codes (`docs/hud.md`, menus).

use std::path::PathBuf;

use gow2_formats::{
    flp::{self, Flp},
    flp_play::{Player, Val},
    wad,
};

fn player() -> Option<Player> {
    let data = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD")).ok()?;
    let m = wad::records(&data).find(|r| r.name == "FLP_HUDA" && !r.data.is_empty())?;
    let mut p = Player::new(Flp::parse(m.data)?);
    p.clear_events();
    let table = wad::records(&data).find(|r| r.name == "MSGS_TXT")?;
    let text: String = table.data.iter().map(|&c| c as char).collect();
    for (name, v) in flp::message_vars(&text[text.find("*1*").unwrap_or(0)..]) {
        p.set(&name, Val::Str(v));
    }
    Some(p)
}

fn run(p: &mut Player, ticks: u32) {
    for _ in 0..ticks {
        p.call_root("SimKeyEvent");
        p.tick(1.0 / 30.0);
    }
}

#[test]
fn the_message_table_fills_the_text_variables() {
    let Some(p) = player() else { return };
    assert_eq!(p.get("PS2_4005").map(|v| v.text()), Some("Pause".to_string()));
    assert_eq!(p.get("PS2_4012").map(|v| v.text()), Some("Quit Game".to_string()));
    // a message of two lines is split into letter-suffixed variables
    assert_eq!(p.get("PS2_4015a").map(|v| v.text()), Some("Would you like to switch to Easy Mode?".to_string()));
    assert_eq!(p.get("PS2_4015b").map(|v| v.text()), Some("(Only combat will be affected by this change)".to_string()));
}

#[test]
fn the_pause_menu_cycles_its_four_choices() {
    let Some(mut p) = player() else { return };
    p.set_num("PS2_PauseMenu_Event", 1.0);
    p.set_num("PS2_EnableButtons", 1.0);
    run(&mut p, 60);
    assert_eq!(p.get_num("PauseMenu_State"), 1.0);
    let mut seen = Vec::new();
    for _ in 0..4 {
        p.call_root("PressDown");
        run(&mut p, 10);
        seen.push(p.get_num("PauseMenu_State") as i32);
    }
    assert_eq!(seen, vec![2, 3, 4, 1], "Continue, Options, Restart, Quit and around");
}

#[test]
fn the_dead_menu_shows_its_choices_after_the_title() {
    let Some(mut p) = player() else { return };
    p.set_num("PS2_DeadMenu_Event", 1.0);
    run(&mut p, 90);
    p.set_num("PS2_DeadMenu_Event", 2.0);
    p.set_num("PS2_EnableButtons", 1.0);
    for i in 0..10 {
        run(&mut p, 10);
        println!("after {} ticks: DeadMenu frame {:?} Opt frame {:?} state {}", (i + 1) * 10, p.instance_frame("DeadMenu"), p.instance_frame("DeadMenu/Opt"), p.get_num("DeadMenu_State"));
    }
    let (opt, _) = p.instance_frame("DeadMenu/Opt").expect("options clip");
    assert!(opt > 0, "the options clip left its hidden frame");
}
