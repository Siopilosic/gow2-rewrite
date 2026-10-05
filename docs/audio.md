# Audio: sound actions, sound banks and samples

Status: first pass 2026-10-03; bank tables decoded 2026-10-03 (§4). Confidence levels follow `docs/confirmed.md`.

## 1. Which sounds Kratos's moves play. CONFIRMED (disc)

Move actions of kinds `0x07` Sound, `0x08` SoundOnEnemy and `0x09` SoundWindow (`docs/kratos-data.md`
§6.5) carry the **hash of a `SND_*` name** at `+0x08`. All 1,090 sound actions of the hero data resolve
through the hash table: **125 distinct sounds**. `tools/move_sounds.py` writes the timeline to
`analysis/dc/R_HERO00/sounds.tsv` (move, kind, trigger, window, sound). Example `MOV_BasicSquare01`:
`SND_WHOOSHMID_F_A` at 0.13, `SND_HERO_ATTKVOC_SHORT` at 0.16, and `SND_ENEMY_GETHIT_L` on the enemy
on hit (trigger 1, `docs/combat.md` §6).

Most used: `SND_HERO_ATTKVOC_SHORT` (76), `SND_WHOOSHMID_F_A` (71), `SND_UPGRADEWHOOSH` (56),
`SND_HERO_ATKVOC_LONG_A` (55), `SND_CHAINSNAP_A` (41), `SND_HERO_GETHITVOC` (26), `SND_BODYFALL_LIGHT` (20).

**Playback path** (code, MEDIUM): `FUN_00219c40(character, hash)` resolves the hash through the
character's virtual `+0x12c` and plays it on the character's sound emitter (`character+0x1a8`,
`FUN_0017d758`, 28 callers). The emitter queues voices; the actual mixing is the 989snd driver on the IOP
(`989NOMID.IRX`), reached by RPC (not read).

## 2. Sound records (SoundServer `0x15`)

| Record | Example | Layout | Level |
|---|---|---|---|
| `SBI_*` (subtype 4) | `SBI_Hero` (R_PERMA, 99,200 bytes) | u16 0x15, u16 4, u32 count, then `count` entries of 0x1c bytes: `{name[24], u32 offset}`; each sample is raw PS-ADPCM (a VAG body without its header) up to the next offset | HIGH (16 samples decode cleanly) |
| `SBP_*` (subtype 0) | `SBP_general` (R_PERMA, 1.16 MB), `SBP_general2` | u16 0x15, u16 0, u32 count, `count` × `{name[24], u32 sound index}` (`SND_*` → index; 295 names), then a preamble `{3, 2, 0x18, header size, sample offset, sample size}` and a **989snd `SBlk` bank** (version 3, name `1NEG`, 295 sounds) whose sample area follows the header | HIGH (structure); ~~the `SBlk` tables are not decoded~~ decoded in §4 (HIGH) |
| `SEM_*` (subtype 5) | `SEM_SNDEmitter`, `SEM_soundEmitter` (248 bytes) | sound emitter objects placed in levels / on characters | not read |

`.VPK` (343 files, up to 9.5 MB) are per-level streams (music and dialogue) and `.VAG` (194) single
streamed files; neither holds the combat effects.

## 3. Exported audio

- **Kratos's voice** (`SBI_Hero`): 16 samples, `tools/sbi_export.py` → `analysis/audio/SBI_Hero/*.wav`:
  `DIEVOC1`, `H_ATTKL1/2` (long attack shouts), `H_ATTKS1..4` (short), `H_DMG1..6` (hurt), `JUMPVOC1/2`,
  `LIFTVOC1`. 0.32–0.96 s each, no clipping, clean decay to silence.
- **Shared effects bank** (`SBP_general`): splitting the `SBlk` sample area at the ADPCM end flags gives 256
  segments, 128 with audio (87 s, longest 3 s), exported to `analysis/audio/SBP_general/` by position.
  ~~Which `SND_*` name plays which sample needs the `SBlk` sound → program → tone tables (open).~~
  Superseded by §4: `tools/sbp_export.py` exports every sound by `SND_*` name at its tone's pitch
  (`analysis/audio/SBP_general_named/`, 1,061 WAVs, 38 MB, plus `sounds.tsv` with each sound's program).
- ~~The sample rate is not in these records; the WAVs use 22,050 Hz (MEDIUM; the tone records of the bank
  carry the real pitch).~~ For `SBP_` banks the tone records carry the pitch (§4.3). For `SBI_Hero` the rate
  is still not stored anywhere found; 22,050 Hz stays MEDIUM.

## 4. The `SBlk` v3 bank. HIGH (structure), MEDIUM (pitch formula)

`tools/sbp_export.py <wad> <SBP_record> <out>` decodes the bank. All offsets are relative to the `SBlk`
magic (record preamble + 0x18).

| Offset | Field |
|---|---|
| +0x00 | `'SBlk'`, +0x04 version 3, +0x08 flags 4, +0x0c name (`1NEG`) |
| +0x16 | u16 sound count (295 in `SBP_general`) |
| +0x18 | u16 grain count (2,077) |
| +0x1c | sound table offset (0x40) |
| +0x20 | grain table offset (0xe14 = 0x40 + 295 × 12) |
| +0x24 | 0x5040 (unknown) |
| +0x28, +0x2c | sample area size (0x10c250, twice) |
| +0x34 | grain data offset (0x4efc = 0xe14 + 2,077 × 8) |

The two computed offsets match the counts exactly, so the table sizes are CONFIRMED.

### 4.1 Sounds, grains, tones

- **Sound** (12 bytes): s8 volume, s8 volume group, s16 pan, s8 grain count, s8 instance limit, u16 flags,
  u32 first grain (byte offset into the grain table). The `SND_*` name table of the record gives the sound
  index (CONFIRMED: names and programs agree, e.g. `SND_FOOTSTEP_DIRT` branches to `..._DIRTSTEP/WALK/RUN/LAND`).
- **Grain** (8 bytes): u32 `type << 24 | argument`, s32 delay (ticks before the grain). A sound is a small
  program of grains.
- **Tone** (24 bytes, grain type 1, argument = offset into the grain data): s8 priority, s8 volume, s8 center
  note, s8 center fine, s16 pan, s8 map low/high, s8 pitch-bend low/high, u16 ADSR1, u16 ADSR2, u16 flags,
  u32 sample offset, u32 sample size. CONFIRMED for offset/size: the 128 distinct (offset, size) pairs tile
  the sample area contiguously and are exactly the 128 audio segments of §3. The layout is the 989snd
  library's tone record (external library knowledge; fields other than offset/size/note/fine are not
  checked against code).

### 4.2 Grain opcodes. HIGH (numbering), names from the 989snd library

The opcode numbering is the 989snd one (1 tone, 7 plugin message, 8 branch, 25 rand_play, 27 rand_pb,
30/31 set register, 34 test register, 35/36 marker / goto marker, 39 play_cycle, ...;
`GRAIN_TYPES` in the tool). The bank's programs read coherently under it:

- `SND_FOOTSTEP_<surface>`: ~~`test_reg(reg 1 == 0..3)`~~ `test_reg` then `branch` to `..._STEP`, `_WALK`, `_RUN`, `_LAND`.
  ~~Register 1 is the gait; the game sets it before playing (writer not found).~~ The argument reads as
  byte 0 = register, byte 1 = comparison, byte 2 = value: `0x10100` = register 0 == 1. The game writes
  the gait to **register 0** (`FUN_0017e7f0(emitter, 0, gait)` in `Character_FootEffect`,
  `docs/effects.md` §3). CONFIRMED.
- `SND_FOOTSTEP_DIRTSTEP/WALK/RUN`: `rand_pb` (random pitch bend, range 100), `rand_play(1 of 2)`, two tones.
  The **same two samples** are pitched 11.0 / 15.6 / 22.0 kHz for step / walk / run.
- **Branch** (type 8): argument → a 0x20-byte record whose +0x0c is the target sound index.
- **Plugin message** (type 7): argument → a 0x20-byte `'DPMS'` record with a name at +0x08:
  - Kratos's voice lines in `SBP_general2`: `SND_HERO_ATTKVOC_SHORT` = 1 of `H_ATTKS1..4`;
    `SND_HERO_ATTKVOC_LONG` = 1 of `H_ATTKL1/2`; `SND_HERO_GETHITVOC` = 1 of `H_DMG1..6`;
    `SND_HERO_DIEVOC` = `DIEVOC1`; `SND_HERO_LIFTVOC` = `LIFTVOC1`. These are the `SBI_Hero` sample names
    (§3), so the voice samples are played by a plugin, not by the bank's own tones. CONFIRMED (names).
  - The non-`_A` voice sounds first do `set_reg_rand` and a `test_reg` that jumps to the end marker, so a
    shout is skipped some of the time (MEDIUM: the exact chance depends on the register semantics).
  - `SND_SHAKE_*` / `SND_RUMBLE_*` are plugin messages named `CSH_*`: **controller vibration is driven
    through sound events** (HIGH).
  - `SND_M_SAVE_MUS`: a music cue (`SAVE_MUS`).

### 4.3 Pitch. MEDIUM

A tone's center note is negative for every tone in `SBP_general` (−68 … −92). Following the 989snd
convention, a negative center note means a 44.1 kHz reference, and an effect plays at note 60:

`rate = 44100 × 2^((60 − |center note| − center fine / 128) / 12)`

This gives 11,010 / 15,571 / 22,020 Hz for the footstep step / walk / run tones (center note −84 / −78 / −72,
fine 3), i.e. 11,025 / 15,590 / 22,050 Hz within a few hertz. The sign of the fine term and the default
note 60 are library conventions not yet checked against the EE/IOP code. `rand_pb` adds a random bend on
top at play time (not applied in the export).

### 4.4 Coverage of Kratos's move sounds

Of the 125 `SND_*` names played by the hero's move actions (§1), **118** exist in sound banks:
93 in `R_PERMA:SBP_general`, 8 in `SBP_general2` (voice plugin messages), the rest in per-level banks
(`SBP_BOG87`, `SBP_PAL17`, `SBP_RHOD25`, ...). Not found in any `SBP_`: `SND_BONECRUNCH`,
`SND_CHAINWHIP_S_A`, `SND_EARTH_RAINH`, `SND_HERO_GETHIT`, `SND_HERO_GETHIT_VOC`, `SND_WALLCLIMB_STEP`,
`WINDTUNNEL_LP` (probably unused or misspelled names in the move data; a missing sound plays nothing).

## Open questions

- ~~`SBlk` v3 layout: sounds (295) → programs → tones (sample offset, root note, pitch, volume, pan, loop).
  This is the 989 Studios library format (external), but its data is needed for the assets.~~ Done (§4).
- ~~How `SND_HERO_*` names map onto `SBI_Hero` samples (random choice among `H_ATTKS1..4`?).~~ Done (§4.2):
  plugin messages naming the samples, chosen by `rand_play`.
- The `DPMS` plugin on the IOP: how it streams `SBI_` samples, their sample rate, the +0x10 byte (0–4).
- ~~Who sets sound register 1 (footstep gait) and the surface choice (`SND_FOOTSTEP_<surface>`).~~ Done: `docs/effects.md` §3.
- Per-level banks: export with `tools/sbp_export.py` when a level's sounds are needed.
- The emitter and voice management (`FUN_0017d758` family), 3D attenuation, the IOP RPC protocol.

## 5. Sound in the Rust port (2026-10-04)

`gow2_formats::snd` reads the same structures as the Python tools (`SBP_*` banks, `SBI_Hero`, PS-ADPCM; a test compares the decoded samples with the exporter's WAV) and plans a
sound: `Bank::plan(name, regs)` runs the program and returns the voices (samples, rate, volume, delay). Handled opcodes: tone, `rand_play` (argument low byte = how many of the next
grains, second byte = how many to play), `rand_pb` (a small random bend; the unit is not confirmed), `branch`, plugin messages that name `SBI_Hero` samples (Kratos's shouts and
cries, played at 22,050 Hz), `test_reg` (equality on a register; the next grain is skipped when it fails, which selects the footstep program by gait), `set_reg`, `stop`. Not played:
loops, markers, child programs (`start_child`), pans and the ADSR envelopes (a layer of some effects is missing, MEDIUM). `gow2_bevy::audio` plays the voices through Bevy's mixer
with a custom PCM source at each tone's own rate.

In `kratos-play` the sounds come from the moves' own actions (kinds 7, 8 and 9 carry the `SND_*` hash, resolved through the hero data's name table: whooshes, shouts, chain snaps,
the enemy-hit sound on hit), footsteps (gait 1 walk, 2 run, 3 landing, `SND_FOOTSTEP_STONE`; every 0.62 s walking, 0.34 s running: a stand-in for the clip's foot-plant events),
the hurt voice and body fall when he is hit and the death cry. The banks are `R_PERMA.WAD` (`SBP_general`, `SBP_general2`, `SBI_Hero`) and the level's `SBP_<level>`. F5 mutes.
Not done: 3D position and distance, music (`.VPK` streams), ambient emitters (`SEM_*`), the surface-dependent footstep choice, the rumble that sounds carry (`SND_RUMBLE_*`).

### 5.1 Pitch follow-up (2026-10-04)
- The SBI_Hero voice samples were played at 22050 Hz and sounded high. Autocorrelation of the samples puts the speaking F0 near 200 Hz at 22050 Hz, which is too high for a man's voice and fits 11025 Hz (F0 about 100 Hz). `SBI_RATE` is now 11025. MEDIUM: nobody has listened against the real game yet.
- Bank tones (SBlk) keep the formula in 4.3. The `[` and `]` keys in `kratos-play` scale all playback pitch at run time (`GOW_PITCH` sets the start value, F5 mutes) so the right factor can be found by ear.
