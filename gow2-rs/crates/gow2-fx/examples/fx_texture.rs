//! Writes a material's texture as a PPM (RGB) and PGM (alpha) next to the given output stem and prints a row of values: `fx_texture <WAD> <MAT_name> <out stem>`
use gow2_formats::{texture::TextureStore, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    if let Some(mb) = recs.iter().find(|r| r.name == a[2] && r.data.len() == 120) {
        let fl = |o: usize| f32::from_le_bytes(mb.data[o..o + 4].try_into().unwrap());
        println!("tint {:.2} {:.2} {:.2} {:.2}, blend word {:08x}", fl(0x60), fl(0x64), fl(0x68), fl(0x6c), u32::from_le_bytes(mb.data[0x38..0x3c].try_into().unwrap()));
    }
    let t = store.material_texture(&a[2]).expect("texture");
    println!("{}x{}", t.width, t.height);
    let mid = (t.height / 2) as usize * t.width as usize * 4;
    let row: Vec<String> = (0..t.width as usize).step_by((t.width as usize / 16).max(1)).map(|x| format!("{:02x}{:02x}{:02x}/{:02x}", t.rgba[mid + x * 4], t.rgba[mid + x * 4 + 1], t.rgba[mid + x * 4 + 2], t.rgba[mid + x * 4 + 3])).collect();
    println!("middle row rgb/alpha: {}", row.join(" "));
    let (mut amin, mut amax, mut rmax) = (255u8, 0u8, 0u8);
    for p in t.rgba.chunks(4) {
        amin = amin.min(p[3]);
        amax = amax.max(p[3]);
        rmax = rmax.max(p[0]);
    }
    println!("alpha {amin}..{amax}, red max {rmax}");
    let mut ppm = format!("P6\n{} {}\n255\n", t.width, t.height).into_bytes();
    let mut pgm = format!("P5\n{} {}\n255\n", t.width, t.height).into_bytes();
    for p in t.rgba.chunks(4) {
        ppm.extend_from_slice(&p[0..3]);
        pgm.push(p[3]);
    }
    std::fs::write(format!("{}.ppm", a[3]), ppm).unwrap();
    std::fs::write(format!("{}_alpha.pgm", a[3]), pgm).unwrap();
}
