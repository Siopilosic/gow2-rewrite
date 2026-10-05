# Rendering: VU1 microcode, lights, fog

_Started 2026-10-02 for the environment track (`environment-audit.md` step 4, "Shading")._

## 1. VU microcode location. CONFIRMED (data + code)

The ELF's 23 `.DVP.overlay..*` sections are **zero-filled placeholders**; `.DVP.ovlytab` (file 0x206218) points into `.data`. The real microcode lives in `.data` as DMA-chained VIF `MPG` packets. The layout of each packet:
- `cnt` DMA tag at MPG−12;
- VIF NOP at MPG−4;
- `MPG num, vuaddr/8`;
- then the code.

`tools/vu_disasm.py --data` extracts them (micro-mode disassembler written from the public VU encoding) → `analysis/vu/vu_<ee>.txt`.

| Program (EE addr) | Size | VU range | Overlay group (sizes match exactly) | Loaded by |
|---|---|---|---|---|
| A 0x002de104 | 0x2c08 | 0x0000–0x2c08 | 56370531 / 3935118283 / 4115049307 | `FUN_0016c3e8` via `FUN_0016fd40(0x2de100, 0x2e0d38, BASE 0, OFFSET 0x119)`, double-buffered |
| **B 0x002e2504** | 0x24d8 | 0x0000–0x24d8 | 56610403 / 1589607611 / 2942285115 | `FUN_0016fec0` (4 callers) via `FUN_0016fd40(0x2e2500, 0x2e4a00, 0, 0)` |
| C 0x002e4a04 | 0x3350 | 0x0000–0x3350 | 4126919515 | `FUN_00178bc8` → `FUN_00178f88(…, 0x2e4a00, …, 0x2e7d88)` |
| D 0x002e7e04 | 0x188 | 0–0x188 | 3528035 | — |
| E 0x002e8b0c | 0x390 | 0–0x390 | 214627 | `VIF0_TADR = 0x2e8b00` (VU0) |
| F 0x002e83cc / 874c / 8a4c | 0x20 / 0xc0 / 0xa0 | 0x400 / 0 / 0x200 | 1740474883 | VU0 helpers |

**`FUN_0016fd40` = VU1 program upload.** It appends a DMA `ref` tag over [start, end), then VIF `BASE` and `OFFSET`, and remembers the current program in `*0x2d7e18`, so an upload is skipped when that program is already resident.

### Program B is the model renderer. CONFIRMED (runtime)
- **Static evidence:** its entry points are `vi01 = 0x000 / 0x155 / 0x2ab` + `xitop`. These are exactly the three batch bases of the MDL_ packets (`models.md`, triple buffering with absolute addresses, BASE/OFFSET 0).
- **Runtime evidence:** in the savestate `tools/pcsx2/sstates/SCUS-97481 (2F123FD8).01.p2s`, `vu1MicroMem` equals program B byte for byte (9,432/9,432). Program A matches 1,464/11,272 (leftover); the rest are absent. The extracted VU memory is in `analysis/runtime/sstate01/`.
- **Output format seen in VU1 data memory:** the program writes over its input with the GS output:
  - a GIF tag at base+0;
  - per vertex: ST/Q floats, RGBA built with the 2²³ float trick (`0x4b0000RR`, colour 128 = 1.0), XYZ2 with ADC in bit 15 of w.

### Program B structure (first reading; offsets relative to the batch base `vi12`)
- Per-object constant block at base+64…85:
  - 64–68: viewport/clip;
  - 69: flags (bit 1, bit 0x10, bit 0x2000 select paths);
  - 70–72: three light directions;
  - 73–75: three light colours;
  - 76: material colour (multiplies 73–75);
  - 77/78: ambient and extra;
  - 79–85: scratch written by the program.
- Matrices at `16..19(vi11)` (`vi11` = header word w + base), a second set for two-bone skinning (vf12–15, weight from `itof15`).
- Colours are scaled by 128 (`loi 128`) before output.
- **Not decoded yet:** the exact per-vertex lighting formula (three directional lights + ambient is the reading of the setup code, MEDIUM) and the inner loops at 0x1f90 / 0x21a0 / 0x2250 / 0x23a8.

### 1.1 Shader variants: program C is a fragment library. HIGH (code + disc survey), 2026-10-02

Program B is only the frame (setup, clipping, kick). Its dispatch reads `69.y` and does `jr vi04` (VU 0x0058–0x0078). That jump goes into a **vertex-loop fragment** taken from program C and kept in a VU1 micro-memory cache.

- **Cache.** `FUN_001788f8` builds a 0x81c-byte manager (`DAT_002d7e1c`). `FUN_00178bc8` reads the 137-word entry table `DAT_002e2200` (offsets into program C's code, which `FUN_00178f88` unpacks from its VIF MPG packets) and sizes each fragment as the gap to the next entry. Fragments upload at VU address `0x49b` + slot × size. `FUN_001789a8(mgr, idx, alt)` uploads on a miss (VIF `MPG`, round-robin slots) and returns the VU entry address. With `alt` set, it adds the fragment's size, so it enters the companion fragment that follows.
- **Selection** in the draw loop `FUN_00173b28` (and the twins `FUN_001759c0`, `FUN_00153668`):
  ```
  v   = MAT+0x40 (| 0x20 if instance byte +0x16 is set)
  v   = v & part+0x14 | part+0x10          // part = MDL kind-0x0e/0x18 header C
  ent = FUN_001789a8(mgr, mgr[(v & 0x7f) + 4], v & 0x20)
  VIF UNPACK V2-16 → base+69:  x = (v | 0x80) << 8  (flag bits),  y = ent  (jump target)
  ```
  So the MAT `+0x40` low byte is a **shader capability mask** and the model part's AND/OR words restrict it. This replaces the earlier reading of `+0x40` as "z-write" (models.md).
- **Variant bits** (read from the fragment code; table layout repeats per 16 entries):

| Bit | Effect in the fragment | Confidence |
|---|---|---|
| 0x20 | companion fragment: no perspective divide, writes vertex colour and clip-space XYZ (feeds program B's clipper) | MEDIUM |
| 0x08 | **per-vertex lighting**: normal (`itof15`) × light matrix `vf16–18`, `max(·, vf11=0)`, colour = `vf29 + vf27·x + vf28·y + vf09·w`, clamped to 255 with `minii I=2²³+255`, stored as RGBA (fragments 0x480, 0xe90…) | HIGH |
| 0x02 | texture coordinates from the transformed normal (`vf19 + vf16–18·n`, xy into the ST slot). Overrides bit 0x08 in the table. Sphere-style env/light map | MEDIUM |
| 0x01, 0x04, 0x10, 0x40 | change fragment choice (UV/Q handling, skinning variants); not decoded | LOW |

- **Disc survey** (`tools/variant_survey.py` → `analysis/vu/variant_survey.tsv`, 49,328 parts):
  - part masks: OR 0x10 / AND `ffffff25` on 42,313 parts. AND 0x25 clears bits 0x08 and 0x02.
  - final variants: **0x31 ×38,046 and 0x30 ×4,637**: all level scenery, skies, water, fire scrolls. These fragments (0x8b8/0x990) transform position and UV only; the vertex colour unpacked by VIF passes through unchanged.
  - 0x7b/0x3b/0x7a/0x3a (bits 0x02|0x08): characters and enemies (`hero`, `zeus`, `kraken`, soldiers, `grifin*`, `colsus*`).
  - 0x38/0x78/0x39 (bit 0x08 without 0x02): N·L-lit props (`Pillar00/01`, `train00`, `rharch00`, `S3Hand*`).
  - 4,663 parts: material slot not resolved by the survey (HUD, shell, combat meshes).
- **Consequence for the environment track. HIGH:** static level geometry is **not lit at run time**. Its colour is the baked vertex colour (× texture, GS modulate 128 = 1.0). The viewer's vertex colour × texture × 2 is therefore the game's formula for scenery. Dynamic light records (§2) affect only parts with bit 0x08/0x02, i.e. characters and a few props.

## 2. Light records (LightServer, id 6). HIGH (disc layout; consumer not read)

84-byte records `PSambLite*`, `PSdirLite*` (plus `LGTX_<level>` context, 48 bytes):

| Offset | Field | Example (RHOD10) |
|---|---|---|
| +0x00 | type 6 | |
| +0x08 | flags: `0x40` ambient; `0x10452`/`0x10456` directional-type | |
| +0x0c | position vec3, w = 1 | (−1794, 3760, −5399) |
| +0x1c | unit direction vec3, w = 1 | (0.126, 0.885, 0.449) |
| +0x2c | RGB colour, + 1 | (0.819, 0.923, 1.0) |
| +0x3c / +0x40 | scale, radius | 2.0 / 64 |
| +0x44 / +0x48 | scale, radius | 2.0 / 80 |

- Ambient `PSambLite1`: colour (0.858, 0.972, 1.0) and (0.22, 2400) twice.
- The positions sit near Kratos's area, so these are local lights with falloff radii. The falloff is MEDIUM; its consumer is not traced.

## 3. Fog: a full-screen depth post-process. HIGH (code), parameters MEDIUM

**Data.** Script records `SCP_*` (server 4, 168 bytes), class `SCR_Fog` / `SCR_LayeredFog` (name at +4):

| Offset | RHOD10 `rhodesFog` | `rhodesFogInt` | BOG10 `FoggyStart` | Meaning |
|---|---|---|---|---|
| +0x34..+0x40 | 0.5, 0.6, 0.65, 0.15 | 0.3, 0.4, 1.0, 0.335 | 0.21, 0.275, 0.26, 0.9 | fog colour RGB + density (MEDIUM) |
| +0x70 | 13500 | 60000 | 60000 | u32: distance or time (UNKNOWN) |
| +0x74..+0x80 | 0.1, 0.1, 10, 1 | same | 0.06, −0.015, 2.9, 1 | UNKNOWN |
| +0x84 | `defFogMat` | | | material (TXR `defFogText` → `GFX_fog` 128×64 8-bpp) |

- `SCR_LayeredFog` (PAL15 `Fog00`) holds four colour/alpha rows at +0x34..+0x6c.

**Code.**
- **Registration.** renMaster's constructor `FUN_0016d740` creates the fog effect (`FUN_00123390`, 0x1a0 bytes, vtable 0x2f3148). It registers the effect as `RFX_Fog` and `RFX_LayeredFog`, and the script classes `SCR_Fog` / `SCR_LayeredFog` with callbacks `FUN_00159ec0` (create: name `FSE_<name>`), `FUN_00284050` (activate), `FUN_00284020` and `FUN_00283fe8`. The effect joins renMaster at order key 0x7100. Bloom, LayeredBloom, FSWarp and CameraFilter are registered the same way.
- **Activation.** `FUN_0015a008(weight 1.0, params, priority)` inserts the parameters into the effect's sorted blend list, so fog settings stack by priority.
- **Per frame** (`FUN_00123c60`, vslot 2), three GS passes:
  1. `FUN_00122a78`: **Z → alpha.** The Z buffer (TBP = ZBUF page) is drawn as a 16-bit texture into `FRAME_2`, using 8-pixel-shifted sprite strips (`TEXA` TA1 = 0x80, FBMSK 0x3fff). Depth bits end up in the frame alpha.
  2. `FUN_00122dc8`: **alpha → fog density.** The frame buffer is read as `PSMT8H` (its alpha byte) through a 256-entry CLUT and written back to alpha only (FBMSK 0xffffff, ZMSK, `ALPHA_2 = 0x88` Cs·As).
     - The vertex alpha is fog `.x` × 128.
     - The CLUT is built in the constructor: grey 0x808080, alpha ≈ 0x80 − i/2 in CSM1 order, a linear ramp. Two copies are double-buffered at +0x130/+0x134.
  3. `FUN_00122498`: **composite.** A full-screen sprite in the fog colour (RGB × 255), with Z test GREATER and blend from `FUN_00123348(mode)`:
     - `0x54` (Cs−Cd)·Ad+Cd: lerp to the fog colour by alpha (normal);
     - `0x58` Cs·Ad+Cd: additive;
     - `0x52` Cd−Cs·Ad: subtractive.
- **Reproduction recipe (MEDIUM until the CLUT index ↔ depth mapping is pinned):** fog = density · ramp(depth bits), then lerp(colour, fogColour, fog).

### 3.1 Script → effect parameter mapping. CONFIRMED (code), 2026-10-02
- The script's params object is the record payload from `+0x24` (P).
  - Evidence: `FUN_00159dd8` reads `P+4` as the effect index into `0x2d7bc8[]`, and the record `+0x28` = 1 is the slot `FUN_00159f18(fog, 1)` registers.
  - The other slots: 0 CameraFilter, 2 FSWarp, 3 Bloom.
- **Per-frame update** `FUN_00123990` (fog vslot):
  1. `FUN_0015a460` advances fades: state 0 = fade in over `[6]` frames with easing `FUN_00145a58`, 1 = full, 2 = fade out, then remove.
  2. `FUN_00273840` blends the weighted entries into the effect block at `+0x40`. The pairwise lerp is `FUN_00123150`; a texture-name change switches at weight 0.5.
  3. Distance: `camera+0x3ac = min(2·(+0x8c), 524287)`. With no fog, 524287.

| P offset | Record | Effect field | Use |
|---|---|---|---|
| +0x10 vec4 | +0x34 | +0x50 | fog colour RGB (×255 on the sprite) + density (w; the effect is on if w > 0) |
| +0x20 / +0x30 vec4 | +0x44 / +0x54 | +0x60 / +0x70 | extra layer colours (`SCR_LayeredFog`) |
| +0x40 | +0x64 | +0x80 | layered path selector (non-zero → `FUN_0012e730` / `FUN_00121fc8` variants) |
| +0x4c u32 | +0x70 | +0x8c | **distance**: camera Z range = 2× (RHOD10 13,500; interiors 60,000) |
| +0x50 / +0x54 / +0x58 | +0x74.. | +0x90 / +0x94 / +0x98 | fog-layer texture scroll u, v and rotation (degrees per time) |
| +0x60 name | +0x84 | +0xa0 | layer material (`MAT_` + name; `FUN_00123b10`), e.g. `defFogMat` |
| +0x78 | +0x9c | +0xb8 | blend mode: 0 lerp `0x54`, 1 add `0x58`, 2 sub `0x52` |
| +0x7c | +0xa0 | +0xbc | CLUT colour alpha (−1 = 0x80) |

- **Camera use of `+0x3ac`** (`FUN_00177440`, viewport setup): Z scale = F/2 and Z offset = F/2 + 2¹⁹ (F = `+0x3ac`). So the fog distance stretches the Z-buffer range, and the fixed CLUT ramp over the Z bits becomes a ramp over that distance. The exact bit selection of the Z→alpha copy (PSMZ16 view of the Z buffer, 8-pixel strips, FRAME psm 2, FBMSK 0x3fff) is MEDIUM: Ghidra mangles the packed 64-bit register values.
- **Disc survey:** 120 of 122 level WADs have fog scripts:
  - `SCR_LayeredFog` mode 0: 95;
  - `SCR_Fog` mode 0: 68;
  - `SCR_Fog` mode 1 (additive): 14;
  - `SCR_LayeredFog` mode 1: 1.
- **In use:** `gltf_export.py` writes `asset.extras.fog[]` per level. The viewer has a fog selector and applies the exact curve of §3.2 with lerp or add.

### 3.2 Exact fog curve. HIGH (code + GS format), near/far MEDIUM, 2026-10-02

**Pass 1 registers** (raw MIPS of `FUN_00122a78`, GIF A+D list):

| Reg | Value | Meaning |
|---|---|---|
| FRAME_2 | FBP = frame, PSM 2 (CT16), FBMSK 0x3fff | 16-bit view of the 32-bit frame; only the high byte of each 16-bit pixel is written |
| ZBUF_2 | ZBP 0x78, PSM 0x31 (Z24), ZMSK 1 | savestate globals `0x363800`/`0x3637fc` |
| TEST_2 | ZTE, ZTST always | |
| TEX0_2 | TBP = ZBP·32, TBW = fbw, **PSM 0x32 (Z16)**, 1024², TCC, DECAL | the Z buffer as a 16-bit texture |
| TEXA | TA0 0, TA1 0x80 | lossless 5551 round trip |
| PRIM | sprite, TME, FST, context 2 | |

- **Strips.** Each strip copies texture columns [x, x+8) to screen columns [x+8, x+16), every 16 pixels. In the GS 16-bit layout, columns x..x+7 of a 16-wide block are the low halfwords of 32-bit pixels and x+8..x+15 are the high halfwords.
- **Result.** The **frame alpha byte receives Z bits 8–15**: `i = (Z >> 8) & 0xff`.

**Depth to index.**
- The projection (`FUN_00176xxx`, camera `+0x388` near, `+0x38c` far) is OpenGL-style.
- The viewport (`FUN_00177440`) sets Z = 2¹⁹ + (F/2)(1 − z_ndc), with F = camera `+0x3ac` = min(2·distance, 524287).
- Hence, for view depth d:
  ```
  Z − 2^19 = F · n · (f − d) / (d · (f − n))
  i        = ((Z − 2^19) >> 8) & 0xff         // wraps only for d < ≈ F·n/65536 (a few units)
  alpha    = 0x80 − (i >> 1)                   // constructor CLUT, FUN_00123390
  fog      = density · alpha / 0x80            // pass 2 vertex alpha = density·128, Cs·As
  colour   = lerp(scene, fogColour, fog)  |  scene + fogColour·fog  (mode 1)
  ```
- **Near and far.** Both savestate viewports have near = −10 and far = −10000; `FUN_001e17e0` derives them as −0.1·k and −100·k. The values in levels are not captured yet (MEDIUM).

**Consequence (RHOD10: D = 13,500, F = 27,000):**
- The fog factor is 0 next to the camera.
- It is about 0.96·density at d = 100 and reaches the full density at the far plane.
- So GoW2 fog is nearly a uniform veil at the script density, with a short near fade. It is not a long linear gradient.
- The distance field mainly sets how far out the near fade reaches.

The viewer now uses this formula (shader `fog_fragment`, `fogFar` = F).

## Next
1. ~~Which Z bits land in alpha~~ (done, §3.2). Remaining: confirm near/far inside a level (in-level savestate). Read the raw MIPS of `FUN_00122a78` for the exact `TEX0`/`FRAME`/`TEXA` 64-bit values, or capture a GS dump of the fog passes in PCSX2.
2. ~~Map script fields~~ (done, §3.1).
3. ~~Program B's per-vertex lighting loop~~ (§1.1: scenery unlit; lit variants found). Open: which EE code fills `vf16–18`/`vf27–29` from the light records (character lighting), and the bit 0x02 normal-map texture.
4. A PCSX2 GS dump of RHOD10 to verify fog colour and density against a frame.
