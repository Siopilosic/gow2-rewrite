//! Sound banks (`docs/audio.md`): `SBP_*` records (a 989snd `SBlk` bank: sounds are small programs of grains, tone grains point at PS-ADPCM samples) and
//! `SBI_*` records (raw PS-ADPCM samples by name, Kratos's voice). Layouts were decoded in `tools/sbp_export.py` and `tools/sbi_export.py`; this is the same
//! reader plus a small planner that turns a sound name into the voices to start.

use std::collections::HashMap;

use crate::{fixed_str, le_u16, le_u32};

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// Decodes PS-ADPCM (16-byte frames: shift and filter, flags, 28 4-bit samples, low nibble first) up to the frame with the end flag.
pub fn decode_psadpcm(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len() / 16 * 28);
    let (mut h1, mut h2) = (0i32, 0i32);
    for (i, frame) in data.chunks_exact(16).enumerate() {
        let (sf, flags) = (frame[0], frame[1]);
        let shift = (sf & 0x0f) as u32;
        let (f0, f1) = FILTERS[((sf >> 4) as usize).min(4)];
        for &byte in &frame[2..] {
            for nib in [byte & 0x0f, byte >> 4] {
                let s = if nib & 8 != 0 { nib as i32 - 16 } else { nib as i32 };
                let mut v = (s << 12) >> shift;
                v += (h1 * f0 + h2 * f1 + 32) >> 6;
                v = v.clamp(-32768, 32767);
                out.push(v as i16);
                h2 = h1;
                h1 = v;
            }
        }
        if flags & 1 != 0 && i > 0 {
            break;
        }
    }
    out
}

/// A tone grain: which sample and how to pitch it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub vol: i8,
    pub note: i8,
    pub fine: i8,
    pub pan: i16,
    /// Sample offset and size (bytes of ADPCM) in the bank's sample area.
    pub offset: u32,
    pub size: u32,
}

impl Tone {
    /// Playback rate in hertz (MEDIUM): a negative centre note marks a 44.1 kHz reference and an effect plays at note 60 (`docs/audio.md` 4.3).
    pub fn rate(&self) -> f32 {
        let base = if self.note < 0 { 44100.0 } else { 48000.0 };
        base * 2f32.powf((60.0 - (self.note as f32).abs() - self.fine as f32 / 128.0) / 12.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GrainKind {
    Tone(Tone),
    /// A branch to another sound of the bank (index).
    Branch(u32),
    /// A plugin message: the name of an `SBI_` sample (`H_ATTKS1`), a shake (`CSH_*`) or a music cue.
    Plugin(String),
    /// Any other opcode of the 989snd grain set with its 24-bit argument.
    Op(u8, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grain {
    pub kind: GrainKind,
    pub delay: i32,
}

#[derive(Debug, Clone)]
pub struct Sound {
    pub name: String,
    pub vol: i8,
    pub pan: i16,
    pub grains: Vec<Grain>,
}

/// An `SBP_` bank.
#[derive(Debug, Clone)]
pub struct Bank {
    pub name: String,
    pub sounds: Vec<Sound>,
    pub by_name: HashMap<String, usize>,
    samples: Vec<u8>,
}

/// An `SBI_` record: samples by name.
#[derive(Debug, Clone, Default)]
pub struct Sbi {
    pub samples: HashMap<String, Vec<i16>>,
}

/// Sample rate of `SBI_` samples. Not stored anywhere found; 22,050 Hz was assumed first and sounded too high. The fundamental of the shouts is 230 to 400 Hz at 22,050 Hz,
/// a woman's range, and 115 to 200 Hz at half that, a man's, with 90 % of the energy below 1 to 1.7 kHz: so 11,025 Hz (MEDIUM, `docs/audio.md` 3).
pub const SBI_RATE: f32 = 11025.0;

impl Sbi {
    pub fn parse(body: &[u8]) -> Option<Sbi> {
        if body.len() < 8 || le_u16(body, 0) != 0x15 || le_u16(body, 2) != 4 {
            return None;
        }
        let count = le_u32(body, 4) as usize;
        let entries: Vec<(String, usize)> = (0..count)
            .map(|k| {
                let e = 8 + 0x1c * k;
                (fixed_str(&body[e..e + 24]), le_u32(body, e + 24) as usize)
            })
            .collect();
        let mut samples = HashMap::new();
        for (i, (name, off)) in entries.iter().enumerate() {
            let end = entries.get(i + 1).map_or(body.len(), |e| e.1).min(body.len());
            if *off < end {
                samples.insert(name.clone(), decode_psadpcm(&body[*off..end]));
            }
        }
        Some(Sbi { samples })
    }
}

impl Bank {
    pub fn parse(rec_name: &str, body: &[u8]) -> Option<Bank> {
        if body.len() < 8 || le_u16(body, 0) != 0x15 || le_u16(body, 2) != 0 {
            return None;
        }
        let count = le_u32(body, 4) as usize;
        let mut names: HashMap<u32, String> = HashMap::new();
        for k in 0..count {
            let e = 8 + 0x1c * k;
            if e + 28 > body.len() {
                return None;
            }
            names.insert(le_u32(body, e + 24), fixed_str(&body[e..e + 24]));
        }
        let pre = 8 + 0x1c * count;
        if pre + 0x18 > body.len() {
            return None;
        }
        let (smp_off, smp_size) = (le_u32(body, pre + 16) as usize, le_u32(body, pre + 20) as usize);
        let bank = pre + 0x18;
        if body.get(bank..bank + 4) != Some(b"SBlk") {
            return None;
        }
        let nsnd = le_u16(body, bank + 0x16) as usize;
        let (snd_off, grain_off, data_off) = (le_u32(body, bank + 0x1c) as usize, le_u32(body, bank + 0x20) as usize, le_u32(body, bank + 0x34) as usize);
        let samples = body.get(pre + smp_off..(pre + smp_off + smp_size).min(body.len()))?.to_vec();
        let i8_at = |o: usize| body[o] as i8;
        let mut sounds = Vec::with_capacity(nsnd);
        let mut by_name = HashMap::new();
        for s in 0..nsnd {
            let o = bank + snd_off + 12 * s;
            let (vol, pan, ng, first) = (i8_at(o), le_u16(body, o + 2) as i16, i8_at(o + 4).max(0) as usize, le_u32(body, o + 8) as usize);
            let mut grains = Vec::with_capacity(ng);
            for j in 0..ng {
                let g = bank + grain_off + first + 8 * j;
                if g + 8 > body.len() {
                    break;
                }
                let w = le_u32(body, g);
                let (typ, arg, delay) = ((w >> 24) as u8, w & 0xff_ffff, le_u32(body, g + 4) as i32);
                let d = bank + data_off + arg as usize;
                let kind = match typ {
                    1 if d + 24 <= body.len() => GrainKind::Tone(Tone {
                        vol: i8_at(d + 1),
                        note: i8_at(d + 2),
                        fine: i8_at(d + 3),
                        pan: le_u16(body, d + 4) as i16,
                        offset: le_u32(body, d + 16),
                        size: le_u32(body, d + 20),
                    }),
                    8 if d + 0x10 <= body.len() => GrainKind::Branch(le_u32(body, d + 0x0c)),
                    7 if d + 16 <= body.len() => GrainKind::Plugin(fixed_str(&body[d + 8..d + 16])),
                    _ => GrainKind::Op(typ, arg),
                };
                grains.push(Grain { kind, delay });
            }
            let name = names.get(&(s as u32)).cloned().unwrap_or_else(|| format!("sound_{s:03}"));
            by_name.entry(name.clone()).or_insert(s);
            sounds.push(Sound { name, vol, pan, grains });
        }
        Some(Bank { name: rec_name.to_string(), sounds, by_name, samples })
    }

    /// The decoded samples of a tone.
    pub fn pcm(&self, t: &Tone) -> Vec<i16> {
        let (o, e) = (t.offset as usize, (t.offset + t.size) as usize);
        self.samples.get(o..e.min(self.samples.len())).map(decode_psadpcm).unwrap_or_default()
    }
}

/// One voice to start: its samples, playback rate in hertz, volume 0 to 1 and delay in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    pub pcm: Vec<i16>,
    pub rate: f32,
    pub volume: f32,
    pub delay: f32,
}

/// What a plan needs from the outside: the registers the game writes (register 0 is the footstep gait, `docs/audio.md` 4.2) and a random source.
pub struct PlanCtx<'a> {
    pub regs: [i32; 8],
    pub rand: &'a mut dyn FnMut(u32) -> u32,
    pub sbi: Option<&'a Sbi>,
}

impl Bank {
    /// Runs a sound's program and returns the voices it starts. Handled: tone, rand_play, rand_pb, branch, plugin voice samples, test_reg (equality on a register),
    /// set_reg and stop. Loops, markers and child programs are not played (MEDIUM: the common effects are covered, the rest only lose a layer).
    pub fn plan(&self, name: &str, ctx: &mut PlanCtx<'_>) -> Vec<Voice> {
        let mut out = Vec::new();
        if let Some(&i) = self.by_name.get(name) {
            self.run(i, ctx, &mut out, 0, 1.0);
        }
        out
    }

    fn run(&self, idx: usize, ctx: &mut PlanCtx<'_>, out: &mut Vec<Voice>, depth: u32, outer_vol: f32) {
        let Some(snd) = self.sounds.get(idx) else { return };
        if depth > 6 {
            return;
        }
        let base_vol = outer_vol * (snd.vol.max(0) as f32 / 127.0).clamp(0.0, 1.0).max(0.05);
        let mut bend = 0.0f32; // semitones
        let mut i = 0;
        let play = |g: &Grain, bend: f32, out: &mut Vec<Voice>, ctx: &mut PlanCtx<'_>, this: &Bank| match &g.kind {
            GrainKind::Tone(t) => {
                let pcm = this.pcm(t);
                if !pcm.is_empty() {
                    let tv = if t.vol > 0 { t.vol as f32 / 127.0 } else { 1.0 };
                    out.push(Voice { pcm, rate: t.rate() * 2f32.powf(bend / 12.0), volume: base_vol * tv, delay: g.delay.max(0) as f32 / 1000.0 });
                }
            }
            GrainKind::Plugin(msg) => {
                if let Some(pcm) = ctx.sbi.and_then(|s| s.samples.get(msg)) {
                    out.push(Voice { pcm: pcm.clone(), rate: SBI_RATE * 2f32.powf(bend / 12.0), volume: base_vol, delay: g.delay.max(0) as f32 / 1000.0 });
                }
            }
            _ => {}
        };
        while i < snd.grains.len() {
            let g = &snd.grains[i];
            i += 1;
            match &g.kind {
                GrainKind::Tone(_) | GrainKind::Plugin(_) => play(g, bend, out, ctx, self),
                GrainKind::Branch(t) => self.run(*t as usize, ctx, out, depth + 1, base_vol),
                GrainKind::Op(typ, arg) => match typ {
                    // rand_play: play `pick` of the next `n` grains
                    25 => {
                        let (n, pick) = ((arg & 0xff) as usize, ((arg >> 8) & 0xff).max(1) as usize);
                        let group: Vec<&Grain> = snd.grains.iter().skip(i).take(n).collect();
                        for _ in 0..pick.min(group.len()) {
                            let k = (ctx.rand)(group.len() as u32) as usize;
                            play(group[k], bend, out, ctx, self);
                        }
                        i += n;
                    }
                    // rand_pb: a random bend of up to `arg` hundredths of a semitone... the unit is not confirmed; a small bend is what the effects sound like
                    27 => bend = ((ctx.rand)(2001) as f32 - 1000.0) / 1000.0 * (*arg as f32 / 100.0) * 0.5,
                    // set_reg: register in the low byte, value in the next
                    30 => {
                        let r = (arg & 0xff) as usize;
                        if r < ctx.regs.len() {
                            ctx.regs[r] = ((arg >> 8) & 0xff) as i32;
                        }
                    }
                    // test_reg: register, comparison, value; the next grain is skipped when the test fails (equality is the only comparison seen)
                    34 => {
                        let (r, val) = ((arg & 0xff) as usize, ((arg >> 16) & 0xff) as i32);
                        if r < ctx.regs.len() && ctx.regs[r] != val {
                            i += 1;
                        }
                    }
                    24 => return,
                    _ => {}
                },
            }
        }
    }
}

