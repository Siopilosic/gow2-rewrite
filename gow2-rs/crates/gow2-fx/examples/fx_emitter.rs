use gow2_fx::bank::Bank;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let b = Bank::load(&std::fs::read(&a[1]).unwrap());
    for n in &a[2..] {
        let e = &b.emitters[n.as_str()];
        println!("{n} subtype {} P {:?}", e.subtype, e.p.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>());
        println!("   matrix {:?}", e.matrix.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
    }
}
