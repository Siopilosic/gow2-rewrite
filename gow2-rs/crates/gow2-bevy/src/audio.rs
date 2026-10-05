//! Sound: the game's own sound banks played through Bevy's audio.
//!
//! `SBP_*` banks (effects) and `SBI_Hero` (Kratos's voice) are read from `R_PERMA.WAD` and the level WAD (`gow2_formats::snd`). A sound is named by the
//! `SND_*` hash a move action carries; [`SoundBoard::play`] runs its program and starts one voice per tone grain. The samples are PS-ADPCM, decoded to PCM and
//! handed to the mixer as a custom [`Pcm`] source at the tone's own rate.

use std::sync::Arc;

use bevy::{
    audio::{AddAudioSource, AudioPlayer, ChannelCount, Decodable, PlaybackSettings, Sample, SampleRate, Source, Volume},
    prelude::*,
};
use gow2_formats::{snd, wad};

/// Mono PCM at a sample rate.
#[derive(Asset, TypePath, Clone)]
pub struct Pcm {
    samples: Arc<[Sample]>,
    rate: u32,
}

pub struct PcmDecoder {
    samples: Arc<[Sample]>,
    pos: usize,
    rate: u32,
}

impl Iterator for PcmDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = self.samples.get(self.pos).copied();
        self.pos += 1;
        s
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.pos))
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.rate.max(1000)).unwrap()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs_f64(self.samples.len() as f64 / self.rate.max(1) as f64))
    }
}

impl Decodable for Pcm {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> PcmDecoder {
        PcmDecoder { samples: self.samples.clone(), pos: 0, rate: self.rate }
    }
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Pcm>();
    }
}

/// The banks and Kratos's voice samples.
#[derive(Resource)]
pub struct SoundBoard {
    banks: Vec<snd::Bank>,
    sbi: snd::Sbi,
    seed: u32,
    /// Master volume, 0 to 1 (F5 mutes in `kratos-play`).
    pub volume: f32,
    /// Pitch multiplier for every voice (`[` and `]` in `kratos-play` change it by a semitone), for tuning the playback rates by ear.
    pub pitch: f32,
}

impl SoundBoard {
    /// Reads every `SBP_*` bank and `SBI_Hero` of the given WADs. `None` when no bank is found.
    pub fn load(wads: &[&str]) -> Option<SoundBoard> {
        let mut banks = Vec::new();
        let mut sbi = snd::Sbi::default();
        for path in wads {
            let Ok(data) = std::fs::read(path) else { continue };
            for r in wad::records(&data) {
                if r.data.is_empty() {
                    continue;
                }
                if r.name.starts_with("SBP_") {
                    if let Some(b) = snd::Bank::parse(&r.name, r.data) {
                        banks.push(b);
                    }
                } else if r.name == "SBI_Hero" {
                    if let Some(s) = snd::Sbi::parse(r.data) {
                        sbi.samples.extend(s.samples);
                    }
                }
            }
        }
        (!banks.is_empty()).then_some(SoundBoard { banks, sbi, seed: 0x1357_9bdf, volume: 0.8, pitch: std::env::var("GOW_PITCH").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0) })
    }

    pub fn has(&self, name: &str) -> bool {
        self.banks.iter().any(|b| b.by_name.contains_key(name))
    }

    /// Plays a sound by name with the gait register (0 step, 1 walk, 2 run, 3 land for footsteps). Returns the number of voices started.
    pub fn play(&mut self, name: &str, gait: i32, commands: &mut Commands, assets: &mut Assets<Pcm>) -> usize {
        let Some(bank) = self.banks.iter().find(|b| b.by_name.contains_key(name)) else { return 0 };
        let mut seed = self.seed;
        let mut rand = |n: u32| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) % n.max(1)
        };
        let mut ctx = snd::PlanCtx { regs: [gait, 0, 0, 0, 0, 0, 0, 0], rand: &mut rand, sbi: Some(&self.sbi) };
        let voices = bank.plan(name, &mut ctx);
        self.seed = seed;
        let n = voices.len();
        if std::env::var_os("GOW_LOGSND").is_some() {
            println!("sound {name} gait {gait}: {n} voices {:?}", voices.iter().map(|v| (v.pcm.len(), v.rate as u32, (v.volume * 100.0) as u32)).collect::<Vec<_>>());
        }
        for v in voices {
            let lead = (v.delay * v.rate) as usize;
            let mut samples: Vec<Sample> = vec![0.0; lead];
            samples.extend(v.pcm.iter().map(|&s| s as Sample / 32768.0));
            let handle = assets.add(Pcm { samples: samples.into(), rate: (v.rate * self.pitch).round().max(2000.0) as u32 });
            commands.spawn((AudioPlayer::<Pcm>(handle), PlaybackSettings::DESPAWN.with_volume(Volume::Linear((v.volume * self.volume).clamp(0.0, 1.5)))));
        }
        n
    }
}


