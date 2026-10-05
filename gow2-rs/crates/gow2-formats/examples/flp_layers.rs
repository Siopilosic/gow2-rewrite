//! Lists the layers of the root timeline of a movie with their instance names, keyframes and characters: `flp_layers <WAD> <FLP name> [name filter]`.
use gow2_formats::{flp::Flp, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let m = recs.iter().find(|r| r.name == a[2] && !r.data.is_empty()).expect("movie");
    let flp = Flp::parse(m.data).expect("parse");
    // flp_layers <WAD> <FLP> [name filter] [clip index | all]: with a clip index the layers of that clip, with `all` every clip that has a layer matching the filter
    let filter = a.get(3).cloned().unwrap_or_default();
    // `flp_layers <WAD> <FLP> uses <clip id>`: where is that clip placed?
    if filter == "uses" {
        let id: u16 = a[4].parse().unwrap();
        let chs: Vec<u16> = flp.chars.iter().enumerate().filter(|(_, c)| **c == (7, id)).map(|(i, _)| i as u16).collect();
        let mut all: Vec<(String, &gow2_formats::flp::Clip)> = vec![("root".into(), &flp.root)];
        all.extend(flp.clips.iter().enumerate().map(|(i, c)| (format!("clip {i}"), c)));
        for (n, c) in all {
            for (i, layer) in c.layers.iter().enumerate() {
                if let Some(k) = layer.iter().find(|k| chs.contains(&k.ch)) {
                    println!("{n} layer {i} name {:?} first at frame {}", flp.string(k.name), k.frame);
                }
            }
        }
        return;
    }
    if a.get(4).map(|s| s.as_str()) == Some("all") {
        for (ci, c) in flp.clips.iter().enumerate() {
            for (i, layer) in c.layers.iter().enumerate() {
                let name = layer.iter().find_map(|k| flp.string(k.name).filter(|s| !s.is_empty())).unwrap_or("");
                if !filter.is_empty() && name.contains(&filter) {
                    let keys: Vec<String> = layer.iter().map(|k| format!("f{}:c{}", k.frame, k.ch)).collect();
                    println!("clip {ci} layer {i} {name:?}: {}", keys.join(" "));
                }
            }
        }
        return;
    }
    let clip = match a.get(4).and_then(|s| s.parse::<usize>().ok()) {
        Some(i) => &flp.clips[i],
        None => &flp.root,
    };
    for (i, layer) in clip.layers.iter().enumerate() {
        let name = layer.iter().find_map(|k| flp.string(k.name).filter(|s| !s.is_empty())).unwrap_or("");
        if !name.contains(&filter) {
            continue;
        }
        let keys: Vec<String> = layer.iter().map(|k| format!("f{}:c{}{}", k.frame, k.ch, flp.chars.get(k.ch as usize).map_or(String::new(), |c| format!("(t{} id{})", c.0, c.1)))).collect();
        println!("layer {i} {name:?}: {}", keys.join(" "));
    }
}
