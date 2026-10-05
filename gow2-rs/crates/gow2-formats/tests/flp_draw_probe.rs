//! Prints the draw list of the HUD's main meter group (debug aid for layout problems): `cargo test --test flp_draw_probe -- --nocapture`.

use std::path::PathBuf;

use gow2_formats::{flp, flp_play::Player, wad};

#[test]
fn print_main_meter_draw_list() {
    let Ok(data) = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let f = flp::Flp::parse(recs.iter().find(|r| r.name == "FLP_HUDA" && !r.data.is_empty()).unwrap().data).unwrap();
    let shapes = flp::parse_shapes(recs.iter().find(|r| r.name == "MDL_HUDA_0" && !r.data.is_empty()).unwrap().data);
    let tags: Vec<(wad::Tag, String)> = recs.iter().map(|r| (r.tag, r.name.clone())).collect();
    let group = flp::texture_group(&tags, "FLP_HUDA");
    let mut p = Player::new(f);
    p.clear_events();
    p.set_num("PS2_HealthMeter_Level", 4.0);
    p.set_num("PS2_MagicMeter_Level", 5.0);
    p.set_num("PS2_HealthMeter_Value", 200.0);
    p.set_num("PS2_MagicMeter_Value", 200.0);
    p.set_num("PS2_MeterBar_Event", 1.0);
    p.set_num("PS2_HealthMeter_Event", 2.0);
    p.set_num("PS2_MagicMeter_Event", 2.0);
    for _ in 0..90 {
        p.call_root("SimKeyEvent");
        p.tick(1.0 / 30.0);
    }
    println!("{}", p.dump(3));
    let fr = p.draw_instance("MainMeterT").unwrap();
    for d in &fr.shapes {
        let sr = &p.flp.shapes[d.shape as usize];
        for (k, it) in shapes[sr.shape as usize].items.iter().enumerate() {
            let (mut x0, mut x1, mut y0, mut y1, mut u1, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MIN, f32::MIN);
            for v in &it.verts {
                let x = (d.matrix[0] * v.x + d.matrix[2] * v.y + d.matrix[4]) / 20.0;
                let y = (d.matrix[1] * v.x + d.matrix[3] * v.y + d.matrix[5]) / 20.0;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
                u1 = u1.max(v.u);
                v1 = v1.max(v.v);
            }
            let tex = sr.items.get(k).map(|i| i.texture).unwrap_or(-1);
            let name = usize::try_from(tex).ok().and_then(|t| group.get(t)).cloned().unwrap_or_default();
            println!("shape B{} mdl{} item {k}: x {x0:.0}..{x1:.0} y {y0:.0}..{y1:.0} uv max ({u1:.2},{v1:.2}) tex {name} cx {:.2?}", d.shape, sr.shape, d.cx);
        }
    }
}
