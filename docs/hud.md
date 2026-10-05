# HUD and UI variables

Status: first pass 2026-10-03 (pass7 decompilation). Structure HIGH; the UI movie side (how the bars
are drawn, `renFlashServer` 0x1b) is not read.

## 1. The UI object and its variable table. HIGH

- The UI object is allocated with `Mem_New(0x2024)` and built by `FUN_001e1cd0` (10,164 bytes, created by
  the engine setup `FUN_0025a1f0`). Its pointer is **`DAT_002d85e4`** (`g_UI`).
- The UI is data-bound: `FUN_001e1cd0` registers several hundred **named variables** with
  `FUN_001e1c40(ui, kind, name, count, address, extra)`. Each binding is a 0x18-byte entry at
  `ui + 0x434 + indexÂ·0x18` (`index` from `FUN_001e1bd0(ui, name)`): `{name, kind, address, count, extra}`.
  Kind 7 = number, 0 = string. The UI movies read and write these by name.
- The variables live inside the UI object itself; the game writes them and then calls
  `FUN_001e5a90(ui)` to notify the UI. Each `*_Event` variable is set to a code (2 = value changed) to
  make the movie react.

**Variables of the in-game HUD** (offset into the UI object):

| Variable | Offset | Written by `FUN_0020cec8` from |
|---|---|---|
| `HealthMeter_Value` | `+0x14` | Kratos's health (int) |
| `MagicMeter_Value` | `+0x18` | the magic meter `0x335840` (int) |
| `HermesMeter_Value` | `+0x1c` | a third meter |
| `GodMeter_Value` | `+0x38` | **meter `0x335848`** (int; the meter that fills on hits, `docs/combat.md` Â§6) |
| `HitCounter_Value`, `Combo_Amount`, `Combo_Message` | `+0x54`, `+0x58`, `+0x5c` | combo counter |
| `HealthMeter_Level`, `MagicMeter_Level`, `HermesMeter_Level` | `+0x60`, `+0x64`, `+0x68` | upgrade levels (`+0x60` = byte `0x335866`) |
| `HealthShard_Count`, `MagicShard_Count`, `CyclopsEye_Count` | `+0x70`â€¦`+0x78` | collectibles |
| `*_Status` (Earth, Electric, Lightning, Medusa, Wind, Bone, Hammer, Olympus, Hermes, IcarusWings, GoldenFleece, GodMode) | `+0x7c`â€¦`+0xa8` | item availability |
| `EnemyMeter_Value1..3`, `EnemyState_Value1..3` | `+0x20`â€¦`+0x34` | boss / enemy health bars |
| power-orb count (`PowerOrb_Event` `+0x318`) | `+0x234` | orbs |
| `HealthMeter_Event`, `MagicMeter_Event`, `HermesMeter_Event`, `PowerOrb_Event`, `HitCounter_Event`, `GodMeter_Event` | `+0x30c`, `+0x310`, `+0x314`, `+0x318`, `+0x31c`, `+0x348` | change notifications |

The same table holds the menus, memory-card screens, challenge results, options (`WideScreen`,
`Vibration`, `InvertPegasus` â€¦) and the statistics (`Stat_PlayerDeaths`, `Stat_EnemyKills` â€¦).

**Naming consequence.** The game's own UI calls meter `0x335848` the **God meter** (`GodMeter_Value`,
`GodMeter_Event`, `GodState_Event`, `GodMode_Status`) and ties its display to the `GodMode` table
(`GBL_Global+0xbc`, `docs/combat.md` Â§4.1): `FUN_0020cec8` compares the meter with the GodMode stage's
`+0x24` threshold and shows the "god" state when `GameFlags_Test(0x200)`. This is the data name; no retail
name is assigned here (`docs/kratos-data.md` Â§6.7).

## 2. HUD update (`FUN_0020cec8`, 2,028 bytes, player side). MEDIUM

Each frame (from the player's update): it writes health, magic and the Hermes meter only when the integer
value changed (and sets the matching event to 2), writes the orb count, computes the God-meter display
state (0/1/2 from the threshold and the god state, `+0x424`/`+0x428`, event `+0x348`/`+0x34c`), and can
hide the bars (`ui+0x1ff0`, `+0x308`; also `SCR_NoPlayerBars` and `SCR_ResetHUD`).

## 3. Game event queue (`FUN_001d0fe0`). MEDIUM

`FUN_001d0fe0(id, a, b, c, object)` appends `{id, a, b, c, object's first handle}` to a list at
`0x003355a0` when `DAT_002d85d8` is set (28 callers; ids `0x1f5`, `0x3fc`â€“`0x425`; e.g. `0x40b` move
started with its name hash, `0x40f` damage with the remaining health). It looks like a gameplay event log
for the challenge/statistics screens. Its consumer is not read.

## Open questions

- ~~The UI movie format and drawing (`renFlashServer`), and which file holds the HUD movie.~~ Answered 2026-10-04: `FLP_HUDA`, see "The movie format" below.
- The consumer of the event queue `0x003355a0`; the meaning of the `HermesMeter` and of `0x0033584c`.

## Reference frame from the real game (2026-10-03)

A PCSX2 screenshot of the first level (Kratos in God armour, `R_HERO01`) shows the in-game HUD: a Blades-of-Chaos-shaped
frame at the top left with a **green health bar** over a **blue magic bar**, a round emblem at its left end, and a red
orb icon with the orb count (318) below it. This matches the variables in section 1 (`HealthMeter_Value`,
`MagicMeter_Value`, the orb count). The HUD movie is `FLP_HUDA` in `R_PERMA.WAD`; its drawing is still not decoded.



## The movie format (decoded 2026-10-04)

Status: HIGH. The earlier reading "a proprietary format that is not decoded, layout a stand-in" is ~~superseded~~: `FLP_HUDA` is read and played, and the
HUD in `kratos-play` is drawn from it. The artwork list of the previous pass still holds: 81 bitmaps `GFX_HUDA000`..`073` and `GFX_HUDA_G000`..`G015`
(without 029-037), each with `PAL_` and `TXR_`; `HUDA010` is the 256 x 96 blade frame, `HUDA_G011` the red orb, `HUDA009` the dark emblem.

**Container.** A `FLP_*` record (server 0x1b) is stored with its pointers zeroed. The loader (`FUN_00159670` and helpers `FUN_001590e8` ..
`FUN_00159578`, decompiled in `0004_0014f358.c`) lays the arrays out one after another, each aligned to 4 bytes, and fixes the pointers up. Header: counts as
u32 at `+0x38` A, `+0x3c` B, `+0x40` C, `+0x44` D, `+0x48` E, `+0x4c` F, `+0x50` G and u16 at `+0x54` H, `+0x56` I, `+0x58` J (string pool bytes); data starts
at `+0x5c`. The walk in `flp.rs` / `tools/flp_decode.py` ends exactly at the record end (407,554 bytes for `FLP_HUDA`).

| Array | Stride | Meaning |
|---|---|---|
| A | 4 | character dictionary `(type, id)`: 1 shape (B), 3 font (C), 4 static text (D), 5 text field (E), 6 button (F), 7 movie clip (G) |
| B | 8 | shape `{ptr, shape id u16, n items u16}`; items 8 bytes `{fill colour 0xAARRGGBB or -1, texture index or -1}` |
| C | 0x24 | font: glyph shape entries (like B), u16 advances, 256 x u16 code to glyph map; `+0x1a` em scale (844) |
| D | 0x1c | static text runs (command stream: font+size, colour, x/y, glyph + advance pairs; `FUN_001550f0`) |
| E | 0x20 | text field: var name `+0`, initial text `+2`, font char `+4`, size (1/1024 em) `+6`, colour `+8`, padding `+0x14`, box xmin/ymin/xmax/ymax `+0x16..`, alignment `+0x1e` (0 left, 1 right, 2 centre) |
| F | 0xc | button with its own clip struct and hit areas (not drawn) |
| G | 0x18 | movie clip: frames `+8`, layers `+10`, frame-info count `+12`, bounds `+14` (s16 x4, twips); the root timeline is one more struct after G |
| H | 0x14 | placement matrix: a, b, c, d as 16.16, then s16 tx, ty in twips; `x' = a x + c y + tx`, `y' = b x + d y + ty`; index 0 = parent's |
| I | 8 | colour transform: 4 x s16 8.8 multipliers r, g, b, a; index 0 = parent's |
| J | bytes | string pool (instance names, labels, variable names, action strings) |

A clip has layers (drawn in order, layer 0 at the bottom). A layer is a list of 10-byte keys `{frame, char, matrix, cxform, name}`; the key shown at frame f is the last
with `frame <= f`, char 0 shows nothing, and a clip instance lives as long as its `(char, name)` stays the same. Frame info entries are `{ptr, frame, n actions, label
string offset}` followed by the action blobs.

**Shapes** are not in the movie. A B entry's shape id indexes the table of the movie's model `MDL_HUDA_0` (u16 count at `+8`, u32 offsets from `+0x18`). A shape is a list of
VIF packets, one per item: `UNPACK S-32` header, `STCYCL`, optional `V2-16` texture coordinates (/4096) or `V2-32` floats, `V4-16` positions (twips, `w` bit 15 = strip
restart), then two `V4-32` GIF/bounds vectors. Items draw as triangle strips. A textured item's texture index counts the `TXR_*` records of the group that precedes the
movie record (`gohuda`: ... `TXR_HUDA_G000`=0, `TXR_HUDA000`=1, `TXR_HUDA001`=2, `TXR_GodOfWarEurope`=3 (the font atlas) ...).

**Actions** are the SWF action set in a compact encoding: opcode byte; `0x96` Push with a type byte (0: u16 string-pool offset, 1: f32); `0x8b` SetTarget / `0x8c` GotoLabel /
`0x81` GotoFrame take a u16; `0x99` Jump and `0x9d` If an s16 offset from the next instruction; `0x9f` GotoFrame2 a flag byte (bit 0 = play); `0x9e` Call; no-argument ops as in
SWF4 (Add 0a, Sub 0b, Mul 0c, Div 0d, Eq 0e, Less 0f, Not 12, StrLen 14, Pop 17, ToInt 18, GetVar 1c, SetVar 1d, StrAdd 21, GetProp 22, SetProp 23, GetTime 34, Stop 07, Play 06).
GotoFrame2 with a number is 1-based (`1 + value/2` for value 0 shows frame index 0, which is empty); GotoFrame is 0-based. Variables are one flat table (`/:Name` and `Name`
are the same).

**How the HUD works** (root frame 7, label `SimKeyEvent`, 4,583 bytes, called by the engine every tick): the game writes `PS2_MeterBar_Event` (0 off, 1 on), `PS2_HealthMeter_Value`,
`PS2_HealthMeter_Level`, `PS2_HealthMeter_Event` (2 = changed), the same for magic, `PS2_PowerOrb_Count` (read directly by the orb text field), `PS2_GodMeter_Value/Event` (0 hide, 1 show),
`PS2_HitCounter_Value/Event` (0 hide, 1 count up, 2 milestone) and every other `PS2_*_Event` starts at -1. The movie then does `/MainMeterT/MainMeter/HealthMeter` gotoAndStop(1 + value/2),
`.../HealthMeterBlackBar` gotoAndStop(label `BarLevel` + level), the magic bar the same (level 0 = no well, else `BarLevel(level-1)`), and the hit counter and God meter (`/TMA`) fade in
and out with their own timelines. Wells: `BarLevel0..4` = frames 51, 102, 152, 202, 252; comparing each well's right edge with the fill's gives capacities 100, 125, 150, 175, 200
(MEDIUM: a geometry fit, `tests/flp_font_probe.rs`; the game's own table is not read).

**Rust.** `gow2_formats::flp` (parser, shape model, texture group), `flp_play` (instance tree, timelines, bytecode, `draw()` -> shapes with matrices and colour multipliers, text fields),
`gow2_bevy::hud` (variables in, `SimKeyEvent` + tick, meshes out on a second 2D camera, centred on the 4:3 stage). Tests: `flp_movie.rs` (layout, counts, shapes, texture group),
`flp_play.rs` (the meters follow the variables). In `kratos-play`: H hurts, M spends magic, O adds orbs, F4 hides the HUD.

**Not done:** text drawing of static text (D) and button states, blend modes other than alpha, GS vertex-colour gradients, the other menus (pause, dead, pickup, save, challenge,
power-up) which are in the same movie, combo milestone messages (`PS2_HitCounter_Event` 2), widescreen. The combo counter rule (shown from the second hit, hidden 2.5 s after the last)
is still a stand-in.

## Layout fix: textures are power-of-two padded (2026-10-04). HIGH
The shape packets store UVs as fractions of the power-of-two padded GS texture, not of the stored bitmap. A 256x96 frame covers v 0..0.75 of a 256x128 texture. The Rust port first sampled the unpadded bitmap, so the blade frame, the magic well and the bars stretched and cut off. `gpu::pad_to_pow2` pads every HUD texture with transparent texels on the right and bottom before upload; the layout now matches the reference frame. The magic well uses level = bar level + 1.

## Menus, static text and the message table (2026-10-04)
The pause menu, the dead menu and the other menus are in the same movie as the HUD and run on the same event protocol. HIGH for the protocol (the port drives them and the screens match the labels), MEDIUM for the layout (some text sits further right than centred; the movie's own matrices are used, so the cause is not known).

- **Message table.** `MSGS_TXT` (R_PERMA, 50,569 bytes, text `*id*` line, then the message) holds every string. The engine puts message `n` into the movie variable `PS2_n`. A message with several lines becomes one variable per line, `PS2_na`, `PS2_nb` ... (`PS2_4015a`, `PS2_4015b` exist in the movie). `flp::message_vars` does this. Square-bracket tags (`[XButton]` button icons, `[PS2_CyclopsEye_Count]` substitutions) are dropped for now. Examples: 4005 Pause, 4006 Options, 4010 Continue, 4011 Restart from last checkpoint, 4012 Quit Game.
- **Static text (`D` array).** Each entry is a command stream (`FUN_001550f0`). A record below 0x80 is a glyph run: count byte, then `(glyph u16, advance i16)` pairs (advance in twips, not scaled). A record with bit 7 set is a command with flags: bit 3 font (character index u16, size u16 in 1/1024), bit 2 colour (4 bytes B, G, R, A, each plus one, over 256), bit 1 pen x (i16), bit 0 pen y (i16). The record after a command is always a glyph run. `flp::decode_static` reads them; the HUD movie has only four (`/10`, `Total PlayTime`, `3`, `R`), so the menu labels are text fields (`E`) fed by the `PS2_n` variables. Static text is not drawn yet.
- **Pause menu protocol.** Set `PS2_PauseMenu_Event = 1` and `PS2_EnableButtons = 1`; the movie plays its open animation (`aOn`) and shows the four choices. The root frames `PressUp`, `PressDown`, `PressLeft`, `PressRight` (labels 12 to 15) move the highlight and play `SND_MM_NAV_UPDN` through the variable `PS2_Sound1`. `PauseMenu_State` holds the choice: 1 Continue, 2 Options, 3 Restart, 4 Quit (cycles). The movie never reports the selection; the engine reads `PauseMenu_State` when Cross is pressed. `ClosePause` (label 16) closes it and sets `PS2_PauseDone`.
- **Dead menu protocol.** `PS2_DeadMenu_Event = 1` shows the title ("YOU ARE DEAD", sound `SND_IG_YOUAREDEAD` through `PS2_Sound2`). Event 2 plays `Wait`, which fills the choice texts and shows them; `DeadMenu_State` is 1 Continue (message 4011), then Return and Exit. Event 0 hides it.
- **Sounds through variables.** The movie plays sounds by writing a name into `PS2_Sound1`, `PS2_Sound2` or `PS2_Sound3` (`SND_MM_POPUP`, `SND_MM_NAV_UPDN`, `SND_MM_NAV_LR`, `SND_MM_BACK`, `SND_IG_YOUAREDEAD`, `SND_IG_POPUP`). The port does not read them yet; `menu_system` plays the same names directly for open and close.
- **VM fix.** The action `Wait` of the dead menu uses `Trace` (opcode 0x26), which the Rust interpreter did not know; an unknown opcode ends the action, so the choices never appeared. 0x26, 0x08, 0x09, 0x20, 0x29 and 0x15 are now handled.
- **In `kratos-play`:** Esc or Start opens the pause menu, d-pad, left-stick flick or arrows move, Cross or Enter selects (Continue resumes, Restart puts Kratos back at the start, Quit exits), Circle, Esc or Backspace goes back. The simulation stops while a menu is open. When health reaches zero the movie shows its title during the fall, then the choices; Continue gets up. `GOW_AUTORESPAWN=1` gets up at once, `GOW_KILL=<s>` kills at that time, `GOW_MENU=pause|dead:<s>[:keys]` opens a menu without input. The confirm step the game asks before restart and quit, the Options screen, the other menus (pickup, save, challenge, power-up) and the button icons are not done.


