//! Disassembles the action lists of a movie's root timeline (or of the clip with a given index): `flp_actions <WAD> <FLP name> [clip index] [text filter]`.
//! Pushes show strings from the pool or floats; only lines that contain the filter (when one is given) are kept, with the label and frame they belong to.
use gow2_formats::{flp::Flp, wad};

fn dis(flp: &Flp, a: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let u16at = |o: usize| u16::from_le_bytes([a[o], a[o + 1]]);
    while i < a.len() {
        let op = a[i];
        i += 1;
        let line = match op {
            0x96 => {
                let mut items = Vec::new();
                while i < a.len() && a[i] <= 1 {
                    let t = a[i];
                    i += 1;
                    if t == 0 {
                        let o = u16at(i);
                        i += 2;
                        items.push(format!("{:?}", flp.string(o).unwrap_or("?")));
                    } else {
                        let f = f32::from_le_bytes([a[i], a[i + 1], a[i + 2], a[i + 3]]);
                        i += 4;
                        items.push(format!("{f}"));
                    }
                }
                format!("push {}", items.join(", "))
            }
            0x8b | 0x8c => {
                let o = u16at(i);
                i += 2;
                format!("{} {:?}", if op == 0x8b { "settarget" } else { "gotolabel" }, flp.string(o).unwrap_or("?"))
            }
            0x81 => {
                let o = u16at(i);
                i += 2;
                format!("gotoframe {o}")
            }
            0x99 | 0x9d => {
                let o = i16::from_le_bytes([a[i], a[i + 1]]);
                i += 2;
                format!("{} {:+}", if op == 0x99 { "jump" } else { "if" }, o)
            }
            0x9f => {
                let f = a[i];
                i += 1;
                format!("gotoframe2 {f}")
            }
            0x0a => "add".into(),
            0x0b => "sub".into(),
            0x0c => "mul".into(),
            0x0d => "div".into(),
            0x0e => "eq".into(),
            0x0f => "less".into(),
            0x12 => "not".into(),
            0x14 => "strlen".into(),
            0x17 => "pop".into(),
            0x18 => "toint".into(),
            0x1c => "getvar".into(),
            0x1d => "setvar".into(),
            0x21 => "stradd".into(),
            0x22 => "getprop".into(),
            0x23 => "setprop".into(),
            0x34 => "gettime".into(),
            0x07 => "stop".into(),
            0x06 => "play".into(),
            _ => format!("op {op:02x}"),
        };
        out.push(line);
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let m = recs.iter().find(|r| r.name == a[2] && !r.data.is_empty()).expect("movie");
    let flp = Flp::parse(m.data).expect("parse");
    let clip = match a.get(3).and_then(|s| s.parse::<usize>().ok()) {
        Some(i) if i != usize::MAX => &flp.clips[i],
        _ => &flp.root,
    };
    let filter = a.get(4).cloned();
    for fi in &clip.frame_info {
        for (n, act) in fi.actions.iter().enumerate() {
            let lines = dis(&flp, act);
            // keep a few lines of context around the matches
            if let Some(f) = &filter {
                let hits: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(f.as_str())).map(|(i, _)| i).collect();
                if hits.is_empty() {
                    continue;
                }
                println!("frame {} label {:?} action {n}:", fi.frame, fi.label);
                let mut last = usize::MAX;
                for h in hits {
                    for i in h.saturating_sub(3)..(h + 4).min(lines.len()) {
                        if last == usize::MAX || i > last {
                            println!("    {:4} {}", i, lines[i]);
                            last = i;
                        }
                    }
                    println!("    ...");
                }
            } else {
                println!("frame {} label {:?} action {n} ({} bytes):", fi.frame, fi.label, act.len());
                for (i, l) in lines.iter().enumerate() {
                    println!("    {:4} {}", i, l);
                }
            }
        }
    }
}
