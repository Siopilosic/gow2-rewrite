//! Lists the records of a WAD whose name contains a substring: tag, size, first bytes.
//! `cargo run --release -p gow2-formats --example rec_probe -- ../extracted/pak/RHOD20.WAD primA44`
use gow2_formats::wad;

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let pat = a.next().expect("name substring");
    let data = std::fs::read(&path).unwrap();
    for (i, r) in wad::records(&data).enumerate() {
        if r.name.contains(&pat) {
            let n = r.data.len().min(if r.data.len() <= 120 { 120 } else { 32 });
            println!("#{i} {:?} {} len {} {:02x?}", r.tag, r.name, r.data.len(), &r.data[..n]);
        }
    }
}

