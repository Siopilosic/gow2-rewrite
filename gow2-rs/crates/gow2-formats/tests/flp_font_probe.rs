use std::path::PathBuf;

use gow2_formats::{flp, flp_play::Player, wad};

fn extent(p: &Player, shapes: &[flp::ShapeMesh], path: &str) -> Option<(f32, f32)> {
    let fr = p.draw_instance(path)?;
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for d in &fr.shapes {
        let sr = &p.flp.shapes[d.shape as usize];
        for it in &shapes[sr.shape as usize].items {
            for v in &it.verts {
                let x = (d.matrix[0] * v.x + d.matrix[2] * v.y + d.matrix[4]) / 20.0;
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

#[test]
fn probe_bar_geometry() {
    let Ok(data) = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_PERMA.WAD")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let f = flp::Flp::parse(recs.iter().find(|r| r.name == "FLP_HUDA" && !r.data.is_empty()).unwrap().data).unwrap();
    let shapes = flp::parse_shapes(recs.iter().find(|r| r.name == "MDL_HUDA_0" && !r.data.is_empty()).unwrap().data);
    let mut p = Player::new(f);
    p.clear_events();
    p.set_num("PS2_MeterBar_Event", 1.0);
    p.tick(0.1);
    // right edge, in the meter group's coordinates, of the health fill at some frames and of the well at its level frames
    let hm = "MainMeterT/MainMeter/HealthMeter";
    let bb = "MainMeterT/MainMeter/HealthMeterBlackBar";
    for fr in [1u16, 10, 25, 50, 51, 75, 99, 100, 101] {
        p.goto_instance(hm, fr);
        println!("fill frame {fr}: {:?}", extent(&p, &shapes, hm));
    }
    for fr in [0u16, 1, 25, 51, 76, 102, 127, 152, 177, 202, 227, 252] {
        p.goto_instance(bb, fr);
        println!("well frame {fr}: {:?}", extent(&p, &shapes, bb));
    }
}
