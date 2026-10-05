//! The sound banks of `R_PERMA.WAD` decode to what the Python exporter wrote (`analysis/audio`), and sound programs plan voices (`docs/audio.md`).

use std::path::PathBuf;

use gow2_formats::{snd, wad};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn bank(recs: &[wad::Record], name: &str) -> Option<snd::Bank> {
    let r = recs.iter().find(|r| r.name == name && !r.data.is_empty())?;
    snd::Bank::parse(name, r.data)
}

#[test]
fn the_general_bank_matches_the_exporter() {
    let Ok(data) = std::fs::read(root().join("extracted/pak/R_PERMA.WAD")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let b = bank(&recs, "SBP_general").expect("SBP_general");
    assert_eq!(b.sounds.len(), 295);
    // a sound played by the first light attack
    let id = b.by_name["SND_BODYFALL_LIGHT"];
    let tone = b.sounds[id].grains.iter().find_map(|g| if let snd::GrainKind::Tone(t) = &g.kind { Some(*t) } else { None }).expect("a tone");
    assert!((tone.rate() - 15571.0).abs() < 2.0, "{}", tone.rate());
    let pcm = b.pcm(&tone);
    // the exporter's WAV (44-byte header, 16-bit mono)
    if let Ok(wav) = std::fs::read(root().join("analysis/audio/SBP_general_named/SND_BODYFALL_LIGHT.wav")) {
        let want: Vec<i16> = wav[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(pcm, want);
    }
}

#[test]
fn programs_plan_voices() {
    let Ok(data) = std::fs::read(root().join("extracted/pak/R_PERMA.WAD")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let b = bank(&recs, "SBP_general").unwrap();
    let mut seed = 7u32;
    let mut rand = move |n: u32| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) % n.max(1)
    };
    // a footstep: the gait register picks the program, one of two samples plays
    for (gait, rate) in [(0, 11010.0), (1, 15571.0), (2, 22020.0)] {
        let mut ctx = snd::PlanCtx { regs: [gait, 0, 0, 0, 0, 0, 0, 0], rand: &mut rand, sbi: None };
        let v = b.plan("SND_FOOTSTEP_DIRT", &mut ctx);
        assert_eq!(v.len(), 1, "gait {gait}");
        assert!((v[0].rate / rate - 1.0).abs() < 0.2, "gait {gait}: {} Hz", v[0].rate);
        assert!(!v[0].pcm.is_empty());
    }
    // a whoosh and a hit sound play something
    let mut ctx = snd::PlanCtx { regs: [0; 8], rand: &mut rand, sbi: None };
    assert!(!b.plan("SND_WHOOSHMID_F_A", &mut ctx).is_empty());
    assert!(!b.plan("SND_ENEMY_GETHIT_L", &mut ctx).is_empty());
}

#[test]
fn kratos_voice_samples_are_named() {
    let Ok(data) = std::fs::read(root().join("extracted/pak/R_PERMA.WAD")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let r = recs.iter().find(|r| r.name == "SBI_Hero" && !r.data.is_empty()).unwrap();
    let sbi = snd::Sbi::parse(r.data).unwrap();
    assert_eq!(sbi.samples.len(), 16);
    assert!(sbi.samples["DIEVOC1"].len() > 5000);
    // the voice line sounds are plugin messages naming those samples
    let b2 = bank(&recs, "SBP_general2").expect("SBP_general2");
    let mut rand = |n: u32| n / 2;
    let mut ctx = snd::PlanCtx { regs: [0; 8], rand: &mut rand, sbi: Some(&sbi) };
    let v = b2.plan("SND_HERO_ATTKVOC_SHORT", &mut ctx);
    assert!(!v.is_empty(), "the shout plays a voice sample");
}
