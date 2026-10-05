# Particles: PTC_ (renParticleSvr 0x11) and FXC_ (EffectsServer 0x19)

_Started 2026-10-02 for the environment track (`environment-audit.md` step 5, particles/FX; ledger as-06)._

## Summary

- A `PTC_` record is a particle **shape**: lifetime, a per-particle VU1 program in data form (eight parameter lists plus a float table), the render routine and the material.
- An `FXC_` record is an emitter, a field or another effect node. Emitters name their particle shape (`particleShape4`).
- The EE spawns particles: it writes a small state per particle (position, birth time, velocity) into a ring buffer.
- VU1 **program A** (EE 0x2de104, already listed in `rendering.md` §1) evaluates every particle each frame from that state and the shape's parameter table, then expands it to a sprite.

## 1. Code map. CONFIRMED (code)

| Function | Role |
|---|---|
| `FUN_0016bf58` | renParticleSvr constructor (vtable `0x2f3768`, server id 0x11) |
| `FUN_0016b780` | per-WAD particle bank (vtable `0x2f3918`); slot 9 `FUN_0016bb10` creates a shape from a `PTC_` record, slot 10 `FUN_0016bb58` resolves links at GroupEnd |
| `FUN_00168f50` | **PTC_ loader**: builds the 0x78-byte shape descriptor (below) |
| `FUN_0016b8e8` → `FUN_00169ac8` | creates a 400-byte particle **instance** for a shape; computes the VU batch size |
| `FUN_001699e0` / `FUN_00169e88` | reserves N particle slots in the instance ring buffer and returns pointers to the state arrays |
| `FUN_00294f50`, `00296280`, `00296970`, `00297060`, `00297750`, `00297e40`, `00298530`, `00298c20`, `00299310`, `00299a00` | ten emitter-type spawn loops (identical 1,776-byte template; they differ in the inlined shape sampler) |
| `FUN_001335f8` | particles to emit this frame: `rate × dt × scale + carry`; the fraction is carried in emitter `+0xb8` |
| `FUN_00135e48` | per-particle generator for the surface emitter (random triangle by area CDF, barycentric point, interpolated normal, speed and spread; see §4) |
| `FUN_0016c3e8` | draw: uploads program A (`FUN_0016fd40(0x2de100, 0x2e0d38, BASE 0, OFFSET 0x119)`, double-buffered), the shape blob and the particle batches |
| `FUN_00132670` | FXC emitter record constructor (subtypes 2 and 5 and nine others; 0xa4-byte objects). It copies record `+0x84..+0xe3` to object `+0x44`, the shape name `+0x54` to `+0x14` and a second name `+0x6c` to `+0x2c` |
| `0x363820[subtype]` | FX subtype factory table, 14 entries (RAM, `ingame1`); vtable slot 3 creates the object (`0x295d48` for subtype 5, `0x295e10` for subtype 2, `0x2957c0` for subtype 0xc fields) |

## 2. Common record header. HIGH (code + survey of RHOD10)

| Offset | Meaning |
|---|---|
| +0x00 | u16 server (0x11 PTC, 0x19 FXC), u16 subtype |
| +0x08 | u16 index, u16 link (`0xffff` = none). Emitter and shape pairs share the index (`FXC_ENTemit15` / `PTC_ENTpart15`: 1) |
| +0x10 | 4×4 row-vector world matrix (rows at +0x10, +0x20, +0x30; translation at +0x40, w = 1). It holds scale (the DPNG rows have length 2) |
| +0x50 | u32 record size |
| +0x54 | name (shape name, e.g. `particleShape4`), 24 bytes |

## 3. PTC_ shape record. CONFIRMED layout (`FUN_00168f50`), meanings as marked

| Offset | Descriptor | Meaning |
|---|---|---|
| +0x6c | +0x40 | s32, −1 seen. UNKNOWN |
| +0x70 | +0x44 | f32 **lifetime** in seconds. Negative = 0 plus flag 0x1000 (immortal, MEDIUM). CONFIRMED by the alpha ramps in §5 |
| +0x74 | +0x48 | s32 (16 seen). UNKNOWN (maximum count or batch hint) |
| +0x78 / +0x7c | +0x4c / +0x50 | −1, −1. UNKNOWN |
| +0x80 | +0x54 | flags. `& 3` + `0x4040` / `0x2000` / `0x40000` pick the render mode (instance `+0x170`; GS alpha register `0x42` values 0x44/0x48/0x42, MEDIUM); `0x8000` = deferred link; `0x10000` = emit at mesh vertices with colour; `0x80000` sets instance `+0xd4` = 3 |
| +0x84 | | count of trailing qwords after the data table (§3.2) |
| +0x8c..+0x97 | +0x5c..+0x64 | 12 bytes. Instance `+0x151` = bytes per particle extra, `+0x152` = **qwords of state per particle** (2), `+0x153 & 1` doubles it. Particles per VU batch = min(125 / qwords, (0x109 − header) / +0x152) |
| +0x9c | blob +4 | **render routine index** into the VU address table `*0x2d7c18` (`0x2e0d98`, 14 entries: 0xe40, 0x10b8, 0x11e0, 0x13d0, 0x1d08, 0x1ed0, 0x2278, 0xf28, 0x16d8, 0x19f8, 0x26d0, 0x2950, 0x2a30, 0x2b18). If not 6, flag 0x100 is set |
| +0x98.. | blob | copied verbatim (size `+0x50 − 0x98`) and uploaded with VIF `UNPACK V4-32` to VU address **0x16** (`0x6c008016`) |

### 3.1 The blob as program A sees it (VU addresses relative to the batch base `vi12`)

| VU qword | Blob offset | Content |
|---|---|---|
| 0x16 | +0x00 | x = render routine address, y = entry called first (`jalr` at VU 0x60), z = data size → `(size >> 4) + 0x1f`, w = state stride |
| 0x17–0x1e | +0x10..+0x8f | **eight lists** of eight u16 (stored interleaved: slot k is short `(k & 3) * 2 + (k >> 2)`). A zero or `-1` ends a list |
| 0x1f.. | +0x90.. | **data table** of vec4 (list entries are qword indices into it; the loader adds 0x1f) |

The eight lists (program A, VU 0x0088–0x04b8, CONFIRMED by disassembly):

| List | VU | Program A action |
|---|---|---|
| 0 | 0x17 | copy particle state qword i (xyzw) into data entry `L0[i]` |
| 1 | 0x18 | copy particle state qword i (.w only) into `L1[i]` |
| 2 | 0x19 | random vec4: `base + range × rand`, reading (base, range) pairs from 0x20 on, into `L2[i]` (xyzw) |
| 3 | 0x1a | the same, .w only |
| 4 | 0x1b | write evaluated output i (xyzw) to the GIF template at `0x222 + L4[i]` |
| 5 | 0x1c | write evaluated output (.w) to `0x222 + L5[i]` |
| 6 | 0x1d | **operator list**: index into the VU op table `*0x2d7c14` (`0x2e0d48`, 20 entries) |
| 7 | 0x1e | operand for each operator: data entry `L7[i]` |

The loader adds 0x1f to lists 0–3 and 7, 0x222 to lists 4–5, and replaces list 6 entries with `table[i] >> 3` (VU code addresses).

**Time registers** (program A VU 0x0250): data entry 0 `.w` receives the particle's birth time from the state. `vf31.x` = **age in seconds**, `vf31.y` = age²/2. `vf31.z/w` are the same for an earlier time; ops 1, 11 and 12 use them for a second (trail) output. HIGH. A particle whose age exceeds its lifetime takes the `fmand 0x10` branch and is skipped.

**Operator table** (`0x2e0d48`; VU byte address → meaning, from program A). Operands are consecutive data entries starting at `L7[i]`:

| Op | VU | Result |
|---|---|---|
| 0 | 0x530 | `q0` (constant) |
| 1 | 0x548 | `q0`; also emits `q0 − q1·z` (trail) |
| 2 | 0x5a0 | `q0 + q1·age` (linear) |
| 3 | 0x600 | `clamp(q0 + q1·age, 0, 255)` (colour) |
| 4 | 0x668 | `q0 + q1·age + q2·age²/2` (ballistic) |
| 5 | 0x6e8 | op 4 with a second (trail) output |
| 6 | 0x768 | indirect: load the qword at address `int(q0.x)` |
| 7 | 0x798 | keyframe table: integer part of `q0.x` selects the pair, the fraction lerps |
| 8 | 0x7d8 | exponential (uses `eexp`; drag or decay). MEDIUM |
| 9, 10, 13, 14 | 0x850, 0x960, 0xb08 | **plane collision**: solves the crossing time against a plane and reflects, up to 10 bounces (`vi05 = 10`). MEDIUM |
| 11 | 0x5c0 | op 2 with a trail output |
| 12 | 0x698 | op 4 with a trail output |
| 15 | 0xd10 | UNKNOWN |
| 16 | 0x580 | `q1 × q0` |
| 17 | 0x638 | `(q1 + q2·age) × q0` |
| 18 | 0x728 | `(q1 + q2·age + q3·age²/2) × q0` |
| 19 | 0xd88 | UNKNOWN |

Outputs land at VU 0x109 onward in list order. Lists 4 and 5 then route them into the vertex template at 0x222: **0x222 = RGBA, 0x223 = position xyz + size (w), 0x224.w = rotation** for the common sprite shapes (HIGH, §5).

### 3.2 Trailing qwords

~~`+0x84` qwords follow the data table (RHOD10: `01000000 0040aa90 31353535 05000000 ...`). They look like GS register data (TEX0-style words); the material is the `MAT_` reference that follows the record in its group (`MAT_comicsmoke`, `MAT_pticleMat3`). Not decoded. MEDIUM.~~ Superseded by §3.3 (2026-10-02): they are the GIF tag template and the corner constants, not TEX0 words.

### 3.3 Render routines and the GIF template. HIGH (program A disassembly + RHOD10 tails)

The `+0x84` trailing qwords are copied to the batch at `vi12 + (vi12+22).z`. Every render routine loads them as `vf16` (or `vf15`), `vf17`, `vf18` and, for routines 8/9, `vf14`, `vf15`:

- **qword 0 is a GIF tag** (PACKED, PRE = 1). Its PRIM field selects the GS primitive and its REGS field lists the per-vertex registers. The routine writes this tag, then the vertex data, once per particle.
- **qwords 1–2** are routine constants: UV corners in 12.4 fixed texels for sprite/quad routines (`0x400` = 64 texels, so `(64,0)`/`(0,64)` on a 64×64 texture), or a second GIF tag plus cos/sin of the fan step for routine 4.

The material is the `MAT_` reference inside the shape's group (`GroupStart, PTC_x, MAT_y, GroupEnd`; e.g. `PTC_ENTpart15` → `MAT_smoke`, `PTC_FBBFPart2` → `MAT_a05Hl10_Firesploch`). HIGH (WAD structure). Emitter and shape refs are also listed under the owning `go` rig node (`gochunks2` → `FXC_ENTemit13..16`, `PTC_ENTpart13..16`), so emitters move with the rig.

Common prologue: `0x0dc8` loads the 4×4 matrix (`vi12+0..3`, world → clip), `vf28`/`vf29` = viewport scale/offset (`vi12+4/5`), `vf19` = per-axis guard-band scale (`vi12+6`, applied by `0x0df0`). After `jalr vi14, vi15` (the op VM evaluates one particle), the routine reads the template at 0x222+ and returns with `jr vi15`. A failed clip test (`fcand`) jumps to `0x0db0`, which drops the particle.

| Index | VU | GS primitive (RHOD10 tag) | Template input | Output per particle |
|---|---|---|---|---|
| 0 | 0x0e40 | POINT (`PRIM 0`, regs RGBAQ, XYZ2) | 0x222 RGBA, 0x223 xyz | tag, RGBA, XYZ |
| 1 | 0x10b8 | SPRITE, TME + ABE + FST (`0x156`) | 0x222 RGBA, 0x223 xyz + size | tag, RGBA, UV0, XYZ(c + s·vf19), UV1, XYZ(c − s·vf19): screen-aligned, unrotated |
| 2 | 0x11e0 | TRIANGLE_STRIP quad | same as 1 | tag, RGBA, 4 × (UV, XYZ): billboard whose up axis follows the projected world up (matrix row 1, normalised; `vf15 = (r1x+r1y, r1y−r1x)/√(1−r1z²)`) |
| **3** | 0x13d0 | TRIANGLE_STRIP, TME + ABE + FST (`0x154`, regs `RGBAQ, (UV, XYZ2)×4`) | 0x222 RGBA, 0x223 xyz + size, **0x224.w angle** | like 2, with the corners rotated by the angle (sin/cos polynomial at VU 0x1578) and radius size·√2. **The common case: 76 of 91 RHOD10 shapes, 83 of 83 in SPIR50** |
| 4 | 0x1d08 | TRIANGLE_FAN, gouraud + ABE (`0x14d`) | 0x222 centre RGBA, 0x223 rim RGBA, **0x224 xyz centre + radius (w)** | screen-space disc: centre vertex, then a second tag (`NLOOP 9`, XYZ2 only) with rim vertices stepped by the cos/sin in qword 2 (0.7071 = 45°, 4 loop passes × 2 + closing vertex = octagon). Glows (`PTC_SGpart`) |
| 7 | 0x0f28 | LINE, gouraud + AA1 (`0x89`) | 0x223 colour A, 0x222 colour B, **0x224 / 0x225 endpoints (xyz)**, 0x224.w max fraction | streak: `t = min(age / (vi12+21).z, 0x224.w)`, both ends are lerps of the two endpoints by t. Sparks (`PTC_particle3/4`) |
| 8 | 0x16d8 | strip quad | 0x222 RGBA, 0x223 xyz + size, **0x224 direction** | axis billboard: one axis = view-space direction, the other = normalise(position × direction); corner weights in `vf14`/`vf15` (tail qwords 3–4) |
| 9 | 0x19f8 | strip quad | 0x222 RGBA, 0x223 xyz + size, **0x224 and 0x225 world axes** | world-oriented quad (both axes × size); flat decals/rings. ZEUS10 uses it twice |
| 5, 6, 10–13 | 0x1ed0, 0x2278, 0x26d0, 0x2950, 0x2a30, 0x2b18 | not read | | not used in RHOD10, ZEUS10 or SPIR50 |

So the template meaning depends on the routine: §3.1's "0x223 = position" holds for routines 0–3, 8 and 9, but routine 4 takes the position from 0x224 and routine 7 uses 0x223 as a colour. The screen size is `size · 2047.5 / w` (the `loi 2047.5` before the divide), with the √2 corner radius in routine 3. MEDIUM: the vertex order inside each strip (UV `(u1,v0), (u0,v0), (u1,v1), (u0,v1)` with the RHOD10 constants) and the GS blend mode, which the tag's ABE bit enables but the ALPHA register (set elsewhere, probably from flags `+0x80`) chooses.

**3.3.1 Screen size of a sprite (2026-10-03).** HIGH (VU code + RAM `ingame2`); MEDIUM where marked.
- **VU (routine at `0x12a8` in `vu_002de104`, the same pattern at the other three `loi 2047.5`).** `vf02` =
  view-projection × position; `Q = vf28.w / vf02.w`; `vf02.xyz = vf28 · clip + vf29 · w`; `vf02.w = size ·
  2047.5`; then everything × Q. The corners are the centre ± `vf15.xy · size · 2047.5 / w`. `vf15` is a
  roll correction from the second matrix row, about (1, 1) for an unrolled camera (MEDIUM).
- **Batch constants come from the view object** (`FUN_00177440`; MEDIUM: the copy from view fields into batch qwords 4–6 in `FUN_0016c3e8` is not traced field by field, the values fit): `+0x330` = (W/2 · viewport width,
  H/2 · viewport height) = **(256, 224)**, `+0x340` = **(2047.5, 2047.5)**, the GS screen centre. So 2047.5
  in the size path is not a screen scale; it makes the sprite half-size `2047.5 · size / w` GS pixels on a
  512 × 448 frame.
- **Field of view** (`FUN_00177210`): `+0x380` = `tan(fov/2) · +0x378` (default `0x3f1135fc` = 0.5672), and
  the projection X scale is `1 / +0x380`. RAM `ingame2` (RHOD10 camera, view object `0x46a230`):
  `+0x380` = 0.5143 (horizontal), `+0x384` = 0.3857 (vertical, ratio 4:3), near/far `+0x388/+0x38c` =
  −10 / −10000.
- **World equivalent.** A world half-extent h projects to `256 · h / (tan(fov_x/2) · w)` pixels, so a
  particle of size s looks like **h = s · (2047.5/256) · tan(fov_x/2) ≈ 8 · tan(fov_x/2) · s** units: 4.11 · s
  with the RHOD10 capture. Sprite size is screen-relative: the particles keep their screen size when the
  game changes the field of view.
- **Viewer.** `particles.js` scales the size by `2047.5/256 · tan(fov_x/2)` of the viewer camera (was 1).
  Check: the RHOD10 wall-break dust (`PTC_ENTpart13`) now matches the debris chunks in size.


### 3.4 Shape flags (`+0x80`) and the GS blend. HIGH (code: `FUN_00169ac8`, `FUN_0016c3e8`)

The loader copies the flags to shape `+0x54` (`FUN_00168f50`). It adds `0x100` unless the render routine field is 6, and `0x1000` when shape `+0x44` is negative.

**Draw pass.** `FUN_00169ac8` stores the pass in instance `+0x170`. The draw function `FUN_0016c3e8` then draws one list per pass, and the per-pass jump table at `0x2e9ed0` sets ALPHA_1 (GS register 0x42, FIX = 0x80):

| Flags | Pass | ALPHA_1 | Blend |
|---|---|---|---|
| `& 0x4040` | 6 | 0x44 | normal, `(Cs − Cd)·As + Cd`; the per-shape override below can change it |
| else `& 0x2000` | 5 | unchanged | whatever was set before. MEDIUM |
| else `& 3 == 0` | 1 | 0x44 | normal alpha |
| else `& 3 == 1` | 3 | 0x48 | **additive**, `Cs·As + Cd` |
| else `& 3 == 2` | 2, or 4 with `0x40000` | 0x42 | **subtractive**, `Cd − Cs·As` |
| else `& 3 == 3` | 0 | 0x44 | normal alpha |

- **Per-shape override.** When `flags & 0x6040` and `& 3 != 3`, the draw writes ALPHA_1 again for that shape: 0 → 0x44, 1 → 0x48, 2 → 0x42.
- **Texture.**
  - `0x80` binds the shape's `MAT_` (`FUN_00174e38` / `FUN_00175530`). Its TEX0_1, TEX1_1, CLAMP_1 and MIPTBP1/2 go out as A+D, with TEST_1 = `0x5360b`: alpha test GEQUAL 0x60, failures write RGB only, z test GEQUAL.
  - Without `0x80`, TEX0 points at a frame-buffer copy and TEST_1 = `0x50000` (z test only). This is probably a screen-distortion effect. MEDIUM.
  - In TEX0, flags `0x400` = TCC (use texture alpha) and flags bits 2–3 = TFX (0 modulate, 1 decal, 2/3 highlight).
- **Depth write.** `0x20000` clears ZBUF_1.ZMSK, so the shape writes depth (`PTC_breakpointy01P`, the wood-chunk points). Every other shape tests depth without writing it.
- **Other bits.** `0x80000` sets instance `+0xd4` = 3; `0x10000` is the vertex emitter; `0x8000` skips instance creation. MEDIUM.

RHOD10 readings: `0x680` / `0x8680` normal blend, textured, texture alpha. `0x681` / `0x6b5` / `0x635` additive (the fireball sprites, glows and sparks). `0x4681` is pass 6, then overridden to additive. `0x20604` writes depth.

## 4. FXC_ emitter record (subtypes 2, 5 and others). Layout CONFIRMED (`FUN_00132670`), meanings MEDIUM

- `+0x54` names the particle shape the emitter feeds; `+0x6c` is a second name (empty in RHOD10).
- `+0x84..+0xe3` is the parameter block.
- `FUN_00135e48` (surface emitter) reads these fields through a pointer at emitter instance `+0xa0`, whose base is not tied to the record yet:
  - `+0x10` speed and `+0x14` speed random (uniform ± half);
  - `+0x18` / `+0x1c` minimum / maximum distance;
  - `+0x20` rate (count = rate × dt × instance scale `+0xb4`, plus carry);
  - `+0x24` random direction;
  - `+0x28` normal speed.

  Speeds and distances are multiplied by 16 (world units). CONFIRMED as code; the record offsets are not.
- ~~With `P` = record `+0x74`: `P+0x10` speed … `P+0x28` normal speed (MEDIUM; fits `FXC_emitter4`).~~ Withdrawn on the same day. A survey of 25 emitters across RHOD10, BOG20 and PAL15 does not fit it. For example, `+0x84` is (0, 1, 0) or (0, 0, 1) on several records, which is a direction rather than a speed.
- **Survey reading** (MEDIUM):

  | Offset | Reading | Examples |
  |---|---|---|
  | +0x84 | direction xyz | (1, 0, 0) default, (0, 1, 0) ColR10 / HSW, (0, 0, 1) DMChd2 |
  | +0x90 | spread (0.1 … 0.5) | HSW 0.5, HSemit 0.1, PCDemitCT 0.35 |
  | ~~+0x94~~ | ~~rate (/s)~~ (speed, see the anchoring below) | ENT 3, PD 40, HSemit 90, BD 7 |
  | ~~+0x98~~ | ~~speed~~ (speed random range, see below) | emitter4 2.5, DMChd2 4, HSemit 5 |
  | ~~+0xa4~~ | ~~1 or 500. UNKNOWN~~ (rate, see below) | |
  | +0xbc / +0xc0 | 2π, 0.5 (constant) | |
  | +0xd4 / +0xd8 | 0.05 / 0.5 / 0.7 and 0 / 4 / 5 / 6. UNKNOWN | |

- **Anchoring, CONFIRMED by code (2026-10-02).** `FUN_00132f08` gives each emitter instance its own 0x58-byte copy of the template object's `+0x44` block (template = instance `+0x1c`, the `FUN_00132670` object) and stores it at instance `+0xa0`. That block is record `+0x84..+0xdb`, so **P = record +0x84**. This supersedes the survey's rate/speed readings for `+0x94`, `+0x98` and `+0xa4`:

  | P | Record | Meaning | Evidence | RHOD10 values |
  |---|---|---|---|---|
  | +0x00 | +0x84 | vec3, (1,0,0) / (0,1,0) / (0,0,1) / (−1,0,0) | not read by the spawn paths found; axis or emitter-type selector. MEDIUM | ENT (1,0,0), ColR10 (0,1,0), FB (0,0,1) |
  | +0x0c | +0x90 | **cone spread**: polar angle = rand · spread · π/2 (1 = hemisphere), azimuth = rand · 2π | `FUN_00133970` (directional spawn). CONFIRMED | ENT 0, FBSemit8 1 |
  | +0x10 | +0x94 | **speed** (× 16 units/s) | `FUN_00133970`, `FUN_00135528`. CONFIRMED | ENT 3, FB 1, PD 40 |
  | +0x14 | +0x98 | **speed random range**: speed = (P10 − P14/2 + rand · P14) · 16 | same. CONFIRMED | ENT 1, emitter5 60 |
  | +0x18 / +0x1c | +0x9c / +0xa0 | **spawn radius** min / max (lerp by rand, × 16) along the same cone direction | `FUN_00133970`. CONFIRMED | 0 everywhere in RHOD10 |
  | +0x20 | +0xa4 | **rate** (particles/s): count = rate · shape scale (`FUN_0016a430`) · instance `+0xb4` · dt + carry (`+0xb8`) | `FUN_001335f8`; the spawn loops skip the emitter when it is ≤ 0. CONFIRMED | SG / SF 1, ENT / FB / ColR10 0 |
  | +0x24 / +0x28 | +0xa8 / +0xac | surface-emitter terms (both ≤ 0 skips a branch in `FUN_00135528`) | MEDIUM | 1 / 0 |
  | +0x38 / +0x3c | +0xbc / +0xc0 | angle range 2π and a factor 0.5 (`sqrt(rand·…) · P3c` in `FUN_00134cb8`) | MEDIUM | 2π, 0.5 |
  | +0x50 | +0xd4 | random multiplier used by the volume/shape spawners (`FUN_00133de0` … `FUN_00135010`) | MEDIUM | 0, 0.5 |

  The spawn position is `emitter matrix · (cone direction · radius) + instance +0x80`; the velocity is `matrix · (cone direction · speed) + instance +0x90` (MEDIUM: `+0x90` is probably the inherited emitter velocity).
  
  **Rate 0 means no continuous emission.** Most RHOD10 emitters (all `ENTemit*`, the fireball and ColR10 emitters) have rate 0 and hang under breakable or scripted `go` rigs (`gochunks2`), ~~so they presumably fire bursts from a trigger, or an `ANM_` FX track (`ANM_fireball03` sits beside the fireball emitters) animates the copied block at runtime. Which one is not settled. MEDIUM.~~ Settled the same day: **an `ANM_` track of type 10 animates the rate.** HIGH.
  - The rig's `ANM_` has one type-10 track per emitter (`ANM_chunks2`: 4 for `ENTemit13..16`; `ANM_fireball03`: 3 for the fireball emitters). Each owns one clip block of f32 keys on slot 8, and slot 8 × 4 = byte 0x20 = P+0x20, the rate.
  - `FUN_00132f08` (a method shared by the base emitter vtable `0x2f2f80` and the ten emitter vtables `0x2f8798`–`0x2f8918`) is the copy-on-write that makes P writable for this.
  - `ANM_fireball03`: rates 50, 150 and 100 /s from frame 0 to 79 (2.6 s), then 0.
  - `ANM_chunks2`: 0, then about 124 /s at frame 1, decaying to 0 by frame 27 (0.9 s). This is the debris puff when the chunks break; the clip plays when the break is triggered.
  - MEDIUM: the f32 key codec for type 10 (same layout as the translation path), and that the player writes to P rather than to another copy.
  
  `+0x84` is not always a unit vector (`FXC_emitter4/5`: (1, 0.25, 0)), so it is probably not a direction; its meaning is open.
- **FX subtypes seen:** 1 (`HSemit`), 2 (`ENTemit`, `SGemit`), 4 (`PCDemitCT`, a curve emitter paired with a subtype-13 `PCDcurveCT`), 5 (`emitter4`, `PDemit`), 12 (fields), 13 (curves).
- ~~**Per-subtype classes (2026-10-03, RAM `ingame2` factory table `0x363820` + pass7).** Each factory entry's vtable~~
  ~~slot 3 builds the template (`FUN_00132670` for every emitter subtype, `0xa4` bytes) and slot 5 builds the~~
  ~~instance (`0xd0` bytes, `FUN_001327a8`) and sets a per-subtype vtable whose spawn routine differs. HIGH:~~

  | Subtype | Instance vtable | Spawn routine | Reading |
  |---|---|---|---|
  | ~~0~~ | ~~—~~ | ~~`FUN_00133748`~~ | ~~base / point (MEDIUM)~~ |
  | ~~1~~ | ~~`0x2f8948`~~ | ~~`FUN_00133bd8`~~ | ~~**omni**: direction = normalised random point of the cube [−0.5, 0.5]³ (spread unused), radius lerp(P18, P1c) × 16, speed (P10 − P14/2 + rand · P14) × 16. CONFIRMED~~ |
  | ~~2~~ | ~~`0x2f8918`~~ | ~~`FUN_00133de0`~~ | ~~directional cone (§4 table)~~ |
  | ~~3~~ | ~~`0x2f87c8`~~ | ~~`FUN_001363f8` / `FUN_00136468` (uses `FUN_0013ec20`)~~ | ~~emitter along a curve (the blade's `FXC_BDEsparkemit` feeding the flames; MEDIUM)~~ |
  | ~~4~~ | ~~`0x2f87f8`~~ | ~~`FUN_00135cd0`~~ | ~~curve emitter (`PCDemitCT`)~~ |
  | ~~5~~ | ~~`0x2f88e8`~~ | ~~`FUN_00134620`~~ | ~~volume / shape (MEDIUM)~~ |
  | ~~6~~ | ~~`0x2f88b8`~~ | ~~`FUN_00134968`~~ | ~~volume (MEDIUM)~~ |
  | ~~7~~ | ~~`0x2f8888`~~ | ~~`FUN_00134cb8`~~ | ~~disc/ring (`sqrt(rand) · P3c`, MEDIUM)~~ |
  | ~~8~~ | ~~`0x2f8858`~~ | ~~`FUN_00135010`~~ | ~~volume (MEDIUM)~~ |
  | ~~9~~ | ~~`0x2f8828`~~ | ~~`FUN_00135368`~~ | ~~surface (`FUN_00135528`, MEDIUM)~~ |
  | ~~10~~ | ~~`0x2f8798`~~ | ~~`FUN_00138d10`~~ | ~~(open)~~ |
  | ~~11~~ | ~~—~~ | ~~`FUN_00139a20`~~ | ~~**trail** object (`docs/effects.md` §5). CONFIRMED~~ |
  | ~~12~~ | ~~—~~ | ~~`FUN_002957c0`~~ | ~~field (gravity/uniform)~~ |
  | ~~13~~ | ~~—~~ | ~~`FUN_00295710`~~ | ~~curve~~ |

  ~~`tools/ptc_export.py` now exports every 228-byte emitter (all but 11/12/13) and tags each with the `go`~~
  ~~effect(s) whose group lists it; the viewer spawns subtype 1 as omni and the rest with the subtype-2 cone~~
  ~~(viewer approximation for 3–10).~~

  **Superseded (2026-10-03, later the same day).** The "spawn routine" column above mixed up three kinds of
  function: `FUN_001363f8` is the `SCR_Mirror` native registration, `FUN_00135cd0` / `FUN_001363c0` only
  build an update kernel, and every shape routine sat one subtype too low. The corrected chain is below.

- **Per-subtype classes, corrected (2026-10-03).** CONFIRMED by code (each link read in pass7 plus RAM).
  The factory table `0x363820` holds one 4-byte class pointer per subtype (RAM `ingame1`). In each class
  vtable, slot 3 builds the template, slot 5 builds the instance and slot 7 creates an update kernel. A
  kernel is a 12-byte list node {prev, next, vtable}; its vtable slot 3 (`+0xc`) is the per-frame emit and
  update loop (about 1,776 bytes each, the same template with a different shape routine inlined).

  | Subtype | Class | Instance vtable | Kernel vtable | Update loop | Shape routine | Reading |
  |---|---|---|---|---|---|---|
  | 0 | `0x2f7648` | — | `0x2f7760` | `0x29a0f0` | — | base (open) |
  | 1 | `0x2f75d0` | `0x2f8948` | `0x2f7748` | `0x299a00` | `FUN_00133780` frame around P+0x00, `FUN_00133970` cone | **directional**: polar `rand · spread · π/2`, azimuth `rand · 2π` |
  | 2 | `0x2f75a8` | `0x2f8918` | `0x2f7730` | `0x299310` | `FUN_00133bd8` | **omni**: normalised random point of [−0.5, 0.5]³, spread unused |
  | 3 | `0x2f7620` | `0x2f87c8` | `0x2f7688` | `0x296280` | `FUN_00135d08` matrix, `FUN_00135e48` | **surface** (mesh, §4.2) |
  | 4 | `0x2f75f8` | `0x2f87f8` | `0x2f76a0` | `0x296970` | `FUN_001353a0` matrix, `FUN_00135528` | **curve** (§4.2) |
  | 5 | `0x2f7580` | `0x2f88e8` | `0x2f7718` | `0x298c20` | `FUN_00133de0` matrix, `FUN_001342f8` | volume **cube**: three `2·rand − 1` |
  | 6 | `0x2f7558` | `0x2f88b8` | `0x2f7700` | `0x298530` | `FUN_00134620` | volume **sphere**: azimuth `rand · sweep`, polar `acos(2·rand − 1)` |
  | 7 | `0x2f7530` | `0x2f8888` | `0x2f76e8` | `0x297e40` | `FUN_00134968` | volume **cylinder**: radius `sqrt(rand)`, height `2·rand − 1` |
  | 8 | `0x2f7508` | `0x2f8858` | `0x2f76d0` | `0x297750` | `FUN_00134cb8` | volume **cone**: `sqrt(rand)` height, `sqrt(rand)·height` radius |
  | 9 | `0x2f74e0` | `0x2f8828` | `0x2f76b8` | `0x297060` | `FUN_00135010` | volume **torus**: tube `sqrt(rand) · P3c` (section radius), angle `2π·rand` |
  | 10 | `0x2f7440` | `0x2f8798` | (`FUN_00281c98`) | — | `FUN_00138d10` | open (one emitter, `FXC_blackemit`) |
  | 11 | `0x2f74b8` | — | (`FUN_00282680`) | — | `FUN_0013a8e0` | trail (`docs/effects.md` §5) |
  | 12 | `0x2f7490` | — | `0x2f7670` | — | — | field |
  | 13 | `0x2f7468` | — | `0x2f7670` | — | — | emission geometry (§4.2) |

  Subtypes 1–4 are Maya's `emitterType` + 1 (directional, omni, surface, curve), and 5–9 are Maya's
  volume shapes in `volumeShape` order (cube, sphere, cylinder, cone, torus). The data agree: 404 of 431
  subtype-1 emitters have a spread, but only 41 of 297 subtype-2 emitters do; no subtype-3 emitter does.

- **4.1 Volume emitters: the Maya volume-emitter parameters (2026-10-03).** CONFIRMED (code + defaults).
  Across 3,340 emitters on 120 WADs, every omni/directional/curve emitter (subtypes 1–4) carries
  P+0x38..+0x54 = (2π, 0.5, 1, 1, 0, 0, 0, 0), which are exactly Maya's volume-emitter defaults (volume
  sweep 360°, section radius 0.5, away from centre 1, away from axis 1, along axis 0, around axis 0, random
  direction 0, directional speed 0). ~~The subtype-5 spawn routine~~ The sphere routine `FUN_00134620` (subtype 6, see the corrected table above) uses them in that sense:

  | P | Record | Maya attribute | Use in `FUN_00134620` |
  |---|---|---|---|
  | +0x00 | +0x84 | direction (1, 0, 0) default | normalised, × P54 (directional speed) |
  | +0x38 | +0xbc | volume sweep (2π) | azimuth = rand × sweep |
  | +0x3c | +0xc0 | section radius (0.5) | torus tube radius (~~subtype 8~~ subtype 9, `FUN_00135010`) |
  | +0x40 | +0xc4 | away from centre | × normalised spawn point |
  | +0x44 | +0xc8 | away from axis | (cylinder-type shapes; not in the sphere routine) |
  | +0x48 | +0xcc | along axis | × axis (0, 1, 0) |
  | +0x4c | +0xd0 | around axis | × normalised (axis × point) |
  | +0x50 | +0xd4 | random direction | × random vector (the "random multiplier" above) |
  | +0x54 | +0xd8 | directional speed | × direction P00 |

  The spawn point is a random point of a unit shape around the local Y axis (sphere: azimuth
  `rand·sweep`, polar `acos(2·rand − 1)`), then the emitter matrix scaled by 16 (`FUN_00133de0` builds
  `diag(16, 16, 16, 1)`). ~~Shapes per subtype: **5 sphere** (code), **6 cylinder** (height `2·rand − 1`),
  **7 cone** (`sqrt` sampling), **8 torus** (tube from P3c), **10 cube** — the last four follow Maya's
  shapes, MEDIUM. Example: the health/magic/weapon orbs are cylinders with "around axis" 0.1 (the swirl)
  and "random direction" 0.01; `gosplash` is a sphere with "along axis" 1–1.6 (the upward throw).~~
  Shapes per subtype, CONFIRMED (each shape routine read from its own kernel, corrected table above):
  **5 cube**, **6 sphere**, **7 cylinder**, **8 cone**, **9 torus**, which is Maya's `volumeShape` order.
  Example: the health/magic/weapon orbs (subtype 6) are spheres with "around axis" 0.1 (the swirl) and
  "random direction" 0.01; `gosplash` (subtype 5) is a cube with "along axis" 1–1.6 (the upward throw).

  **Viewer.** `tools/ptc_export.py` now writes the whole block as `P`, and `analysis/levels/particles.js`
  spawns ~~subtypes 5–8 and 10~~ subtypes 5–9 (fixed 2026-10-03) this way. The velocity sum is multiplied by the emitter speed (P10 ± P14/2) × 16,
  as Maya's speed attribute does (MEDIUM: that multiply is not seen in `FUN_00134620`). All 121 level
  exports and the two gallery sets were re-exported.
- **4.2 Surface and curve emitters, and the emission geometry (2026-10-03).** CONFIRMED by code unless marked.
  - **Geometry record (subtype 13, 136 bytes, built by `FUN_00132588` into a 0x34-byte object).** `+0x10..+0x4c`
    matrix, `+0x54` u32 flags, `+0x58` char[24] data name, `+0x70` char[24] shape name. `flags & 3` = 0 looks
    the data up as a curve (`FUN_0014b9d0`, `NCV_`), = 1 as a mesh (`FUN_0014b980`, `MSH_`). Across all WADs:
    222 curve and 387 mesh references, against 223 subtype-4 and 391 subtype-3 emitters.
  - **Emitter link.** A subtype-3/4 emitter names the geometry's shape name at record `+0x6c`
    (`FXC_LSemit` → `LSgeomShape1`). All 227 such emitters in the level and gallery exports resolve. HIGH.
  - **`MSH_` (WAD tag 9).** `+0x00` u32 vertex count, `+0x04` u32 triangle count, `+0x08`/`+0x0c` runtime
    pointers (`FUN_0014b980` sets them to the vertex and triangle arrays), `+0x10` vertices of 0x18 bytes
    (position xyz, normal xyz), then triangles of 8 bytes: three u16 vertex indices and a u16 cumulative
    area fraction × 65535 (the last is 65535).
  - **`NCV_`** is the same cubic-curve format as the camera rails (`docs/camera.md` §4.4).
  - **Surface spawn (`FUN_00135e48`).** Pick the first triangle whose cumulative value ≥ `rand · 65535`
    (area-weighted), take a uniform barycentric point (`u + v > 1` folds to `1 − u`, `1 − v`) and the
    interpolated normal. Direction = normal × P+0x28 + normalise(normal × random[−0.5, 0.5]³) × P+0x24;
    these are Maya's surface-emitter **normal speed** and **tangent speed**. When either is non-zero the
    point is pushed out along the direction by `lerp(P18, P1c) × 16` and the velocity is the direction ×
    speed `(P10 − P14/2 + rand · P14) × 16`; there is no spread cone. The routine also returns the
    world normal (third output).
  - **Curve spawn (`FUN_00135528`).** `t = rand · 0.99999 · knot[n−1]`, segment = first knot ≥ t,
    P(t) = [t³ t² t 1]·M_i / w. Tangent = [3t² 2t 1 0]·M_i (not divided by w). If P+0x28 (normal speed)
    > 0: direction = normalise(normalise(T × random) × P+0x28 + normalise(T) × P+0x24); else, if
    P+0x24 > 0, direction = normalise(T); else zero. The instance vector at `+0xd0` (subtype-4 instances
    are 0x120 bytes) is added, then the radius push-out, then the spread cone (`rand · spread · π/2`,
    as subtype 1) or the plain direction, × speed.
  - **Frames.** Both routines build their spawn matrix from the geometry object (instance `+0xac`) and its
    owner's world matrix (instance `+0xa8`, `FUN_00135d08` / `FUN_001353a0`) instead of the emitter matrix,
    and there is no ×16 scale: geometry coordinates are game units. The curve result goes through a second
    instance matrix at `+0xe0..+0x11f` plus `+0x80`/`+0x90`. MEDIUM: what `+0xd0` and `+0xe0` hold.
  - **Example.** `gomaiblade` (`R_WEAPON0_0`): `FXC_BDEsparkemit` is a surface emitter on `BDepoly6Shape`
    (38 vertices, 18 triangles) with normal speed 0.5, so the flames leave the blade's surface. Its geometry
    matrix carries a scale of 64 (blade space, undone by the parent joint). `goicaruswings`: feathers from
    the wing meshes (`WF*GeomShape`, normal speed 1).
  - **Viewer.** `tools/ptc_export.py` exports the geometry as `geoms` (curve segments and knots, or mesh
    vertices and triangles) and the emitter's `geom` name; `analysis/levels/particles.js` spawns subtypes 3
    and 4 as above, in the geometry's frame under the emitter's parent. The `+0xd0` vector and the second
    matrix are not applied (viewer choice). Subtype 1/2 directions were swapped in the viewer and are fixed.
- **4.3 Attach joint and animation channel of an FX node (2026-10-03).** CONFIRMED by code + data.
  - Every FX record (emitters, fields, geometry) starts with the common node header read by `FUN_0013bb10`
    and `FUN_0013bd08`: record `+0x08` s16 → instance `+0x60`, record `+0x0a` s16 → instance `+0x62`.
  - **`+0x08` = parent joint** of the owning object's skeleton, −1 = none. `FUN_0013c010` (node world
    matrix) multiplies the local matrix (instance `+0x20`, from record `+0x10`) by joint matrix
    `[+0x60]` (0x40 bytes each) of the owner's skeleton (owner `+0x104`); −1 uses the local matrix as world,
    −2/−3 are owner-root variants. Data: 99 % of 11,500 rig-group FX records have `+0x08` < the rig's joint
    count.
  - **`+0x0a` = animation channel** (ANM block index), 0xffff = none. `FUN_0013be80` walks the owner's
    animation channel list (owner `+0x104` → `+0x8c` → list at `+0x20`) for the entry whose `+0x1c`
    equals instance `+0x62`; `FUN_00132f08` then points that channel at the emitter's private copy of P
    (writes type 0x16), so the channel animates P (the rate at P+0x20, §4). Data: 7,575 of 7,928 non-0xffff
    values fall on a type-10 ANM block. ~~The exporter's rule "the i-th type-10 track drives the i-th FXC
    listed in the rig group"~~ was wrong for 3,665 of them.
  - **Examples.** RHOD10 `breakWallAA1` / `chunks`: each `FXC_ENTemit*` hangs on its own debris chunk
    joint (2, 3, 7, 8, 30, 34), so the dust follows the falling pieces. `fallingDebris`: joint 136.
  - **Viewer.** The exporter writes `joint` and picks the rate track by block index; `particles.js`
    parents each rig emitter to node `<rig>:j<joint>`. 755 level emitters moved off the root joint, and RHOD10
    plays 55 emitters instead of 38.
- Subtype 0xc (`FXC_uniformField*`, `FXC_ColR10grav*`) is a field: 180 bytes. `+0x54` u32 6, `+0x58` f32 20 (magnitude), `+0x5c` 1.0, then a direction (0, −1, 0) and an angle 2π, 0.5. MEDIUM (Maya uniform/gravity field). The ColR10 shapes bake gravity as op 4's `q2 = (0, −112, 0)` anyway.

## 5. Validation (RHOD10)

| Shape | Lifetime +0x70 | Colour op (op 3, alpha) | Alpha at end of life |
|---|---|---|---|
| `PTC_DPNGpart.0` | 2.0 s | 8.43 − 4.215·age | 0.00 |
| `PTC_ENTpart15` | 1.0 s | 18.97 − 18.97·age | 0.00 |
| `PTC_ColR10part` | 1.8 s | 21.07 − 11.71·age | −0.01 |

All three reach zero alpha exactly at the record lifetime, which confirms `vf31.x` = age in seconds and the lifetime field.

Typical smoke program (`DPNGpart`, render routine 3 = VU 0x13d0):
- L0 = [11, 12]: state → position and velocity.
- L1 = [0]: birth time.
- L3 = [13..16]: random size, growth, angle and spin from the (base, range) pairs at entries 1..8 (size 10..30, growth 6.9..11.5, angle −π..π, spin −π..π).
- Ops: 3 (colour), 2 (position = p + v·age), 2 (size), 2 (angle).
- L4/L5 route these to RGBA, position + size and rotation.

## Next

1. ~~Confirm the emitter parameter block anchoring on a directional emitter (subtype 2)~~ (done, §4: P = record +0x84). Still open: the ten spawn-loop variants versus FX subtypes. ~~The burst path for rate-0 emitters.~~ (done: type-10 `ANM_` tracks animate the rate, §4).
2. ~~Render routines (VU 0x13d0, 0x16d8 …): sprite expansion, UV and texture.~~ Done (§3.3) for 0–4 and 7–9; 5, 6 and 10–13 are unread and unused in three WADs.
3. ~~The trailing qwords and the MAT_ binding~~ (done, §3.3); ~~the render-mode flags (`+0x80 & 3`) and the GS ALPHA blend they select.~~ (done, §3.4).
4. Op 15 and op 19, and the exact collision ops.
5. ~~A disc survey of op usage, then a viewer prototype (CPU evaluation of the same op list in three.js).~~ Op usage in three WADs: ops 0, 2, 3, 4 and 5 only (plus op 19 three times in RHOD10); a disc-wide survey is still open. Viewer prototype done, see §6.
6. ~~Placement of `go` subtype-3 rigs (shared with `animation.md`): rig-attached emitters are drawn at their rig, which still sits at the origin.~~ Done (`animation.md`, go node placement): rig emitters now appear at their objects. Still open: ~~the emitter attach joint~~ (done, §4.3)~~, and the size-to-world scale (`2047.5 / w` and the viewport scale at batch `+4`)~~ (size done, §3.3.1). The batch header (matrix rows, `vf28` viewport scale, `vf29` offset, `vf19` guard band) is filled per draw from the view object (`pauVar40[0x14..0x17]`, `[0x30..0x35]`, `[0x38]` in `FUN_0016c3e8`): runtime camera fields. The size scale therefore needs a camera capture from an in-level savestate.

## 6. Viewer playback (2026-10-02)

`tools/ptc_export.py <wad> <level>_gltf` writes `particles.json` (shapes with their raw lists and data table, emitters with P fields, rig and rate track) and `tex/ptc_<MAT>.png`. `analysis/levels/particles.js` plays them in the level viewer ("particles" checkbox):
- At spawn: the cone direction and speed (§4), then state copy (L0/L1) and random ranges (L2/L3) into a copy of the data table.
- Every frame: the operator list (L6/L7) at the particle's age, routed by L4/L5 into the template.
- Drawing: quads with the shape's `MAT_` texture and the §3.4 blend. Colour and alpha are vertex/128 (GS modulate).
- Rate tracks follow the rig's clip in the animation mixer, so one-shot rigs only emit when "one-shot clips" is on. A world emitter with a track loops its own clock with a 2 s gap.

Checked in RHOD10: 38 emitters load. The world-placed `FXC_ColR10emit*` dust clouds draw at (−1560, 3860, −5520) with the smoke texture, and the console shows no errors.

Viewer approximations, all marked in the code:
- ~~size is drawn as a world half-extent;~~ size × 8 · tan(fov_x/2) (§3.3.1);
- routines 2, 8 and 9 are drawn as camera billboards, and routine 7 as small billboards;
- an emitter in a model-less group within 50 units of the origin (the fireballs) is skipped as script-spawned;
- ~~rig emitters hang under the root joint.~~ rig emitters hang under joint `+0x08` (§4.3).

## 7. Rust port: `gow2-fx` and `gow2-bevy::particles` (2026-10-04)

The viewer's playback (§6) is ported to Rust: `gow2-fx` (no engine types) loads the shapes, emitters, geometry and effect groups of a WAD (`bank.rs`) and plays them on the CPU (`play.rs`); `gow2-bevy::particles` draws the sprites as camera-facing quads, one dynamic mesh per shape, with the shape's `MAT_` texture and the §3.4 blend. `kratos-play` loads the banks of `R_M_EARTH0`, `R_M_MEDUSA0`, `R_M_LGHTN2`, `R_M_ELCTRC0` and `R_M_WIND0`.

**Effect groups.** An unnamed rig record (header 1, 1) followed by empty references to `MDL_`, `ANM_` and `FXC_` records is one effect the game spawns by name, under the `go` node that holds it. The magic WADs have these (`examples/fx_groups` lists them): `goearthstomp` (with the `earthStomp` model and five emitters on its joints), `goearthrockhit`, `goearthrainhit`; `gomedusaflash`, `gomedusanuke`, `gomedusabomb`, `gomedusabombhit`; `golghtnhide`, `golghtnwrist`, `golghtnelbow`, `golghtnshoulder`; `goelectriccore`, `goelectricexplode`; `gowindgust`, `gowindgustchargeup`, `gowindgusthit`, `gowindblowhit`, `gowindtornado`, `gowindtempest`. The rate of an emitter is its type-10 animation track (record `+0x0a` names the block, slot 8 holds the keys, §4); effects without a clip emit at the record's rate for as long as the caller wants.

**What was decided in the port (all MEDIUM unless marked).**
- **Velocity and rig scale.** The earthStomp rig animates the scale of the joint the torus emitter hangs under (0.43 at the start, 8.75 after 0.3 s), which spreads the debris ring like the shock wave (the model's own chunks reach 256 units). A velocity rotated by that scaled matrix flung the debris 4,000 units a second; velocities use the rotation of `local * joint` without its scale and then the placement's scale, positions use the whole matrix. The effect is the same size as the model's chunks after this.
- **Size.** Sprite size is screen-relative (§3.3.1): the front end multiplies it by `SIZE_UNIT * tan(fov_x / 2)` of its own camera (`gow2_fx::play::SIZE_UNIT`, 8 for the game's 512-pixel frame) and, new, by the scale of the placement the particle was born under, so an effect started at half size gets half-size puffs.
- **Orientation.** The Medusa flash, the wind gust and the bomb shoot along the effect's local -z (like the effect models); the others stand upright (+y).
- **Subtractive shapes** (`Cd - Cs * As`: the dark specks of the wind hit and tornado) are not drawn: the standard material has no such blend and multiplying draws black blocks.
- **Disc shapes** (render routine 4: the flash, the bomb rings) use a generated soft disc texture; the game's gouraud fan also fades its colour from the centre colour to a second rim colour, which is not done.
- The per-particle cap is 1,500 per shape (the flash asks for 5,000 particles a second).

**Hooks in the magic** (`gow2-kratos::magic`): `MagicSys::take_fx()` returns the effects that started (every blast with an `effect`, the Medusa flash) and `following()` those that follow a moving shot or core (`gowindgust`, `gowindtornado`, `gowindtempest`, `gomedusabomb`, `goelectriccore`); the front end starts them and keeps them in step with `Particles::sync_follow`. The blast spec carries the effect name and the radius that effect covers at its own size (the stomp: 256 units, from the model).

**Seen in play (saved frames of the game window):** the earth stomp (a dust cloud with debris and a flash), the Medusa flash (a red jet), the bomb (a pink glow), the electric core (a blue haze with arcs) and the wind magic (white clouds). Not seen against the game: how they compare in size and brightness with the retail effects.

**Not done:** `golghtnhide`, `gowindgustchargeup`, `gowindgusthit`, `gowindblowhit` (no hook yet), surface and curve emitters are ported but unchecked (the magic WADs use them only in the lightning body effects), the fields (`FXC_*grav`) are ignored (the shapes carry their gravity in the program), the unread render routines 5, 6, 10 to 13, and the effects of the other WADs (blade flames, enemies, levels).

**Tools:** `crates/gow2-fx/examples`: `fx_groups` (effect groups and emitters of a WAD), `fx_run` (runs an effect without a renderer and prints counts, sizes, alpha and reach per shape; also the joint scales of its rig), `fx_emitter` (the `P` block and matrix of emitters), `fx_texture` (a material's texture as PPM and PGM). `kratos-play`: `GOW_PFX=<effect>[@scale[@fwd]]` starts an effect every two seconds beside Kratos, `GOW_PFXDEBUG=1` draws the sprites as magenta quads, `GOW_PFXBLEND=1` and `GOW_PFXNOTEX=1` change the material, `GOW_LOGFX2=1` also prints the sprite count.

### 7.1 Corrections after the first look (2026-10-05)
- **Size unit divided by 16. MEDIUM (one comparison with the retail game).** Section 3.3.1 reads the VU's `size * 2047.5 / w` as GS pixels; the VU converts positions with `ftoi4`, so the number is in 1/16 pixel, and the world half extent of a size-1 particle is `2047.5 / 16 / 256 * tan(fov_x / 2) * w` = 0.257 units with the game's field of view, not 4.11. Evidence: a PCSX2 frame of the opening (savestate 02) shows the glow of the blades' flames about 2 units across, and the flame particles (`flame3`, size 4 to 7) come out at 1 to 2 units with the corrected unit and at 18 to 31 (bigger than Kratos) with the old one; the lightning on his arms (`LCpart`, size up to 10, on a 5.5-unit upper arm) agrees. The earth stomp's dust shrinks to puffs of about 10 to 20 units as a result. The earlier sentence "Check: the RHOD10 wall-break dust matches the debris chunks" of section 3.3.1 was a look at a viewer and is superseded.
- **The lightning on Kratos's arms (`golghtnshoulder`, `golghtnelbow`, `golghtnwrist`).** `SCR_Lightning` attaches six effects to joints found by name; the data fit lHumerus/rHumerus, lRadius/rRadius and lWrist/rWrist: the curves of the first two run along +y for 5.5 and 6.9 units, which are the lengths humerus to radius (5.6) and radius to wrist (6.9) in the rig, and the wrist effect is a point emitter. The effect's root is the joint's world matrix times Kratos's placement. They run while the Lightning script runs (`MagicSys::body_fx_on`). `GOW_LBODY=1` keeps them on to look at them. Seen in play: blue-white crackles at both shoulders, elbows and wrists.

### 7.2 Size unit again, and the blades' flames (2026-10-05)
- **Size unit: `size * tan(fov_x / 2)` world units. MEDIUM.** The first port (4.11 per size) was far too big, the second (0.257, section 7.1) a little small: in a PCSX2 frame the cyan glow at a blade's handle is about 1.1 units across its radius and `EGpart` (size 1.6) comes out at 0.8 with the third reading. The VU's batch constant `vf19` scales the half extent by the guard-band factor; taking it as 256 / 2048 gives `size * 256 / w` pixels, which is `size * tan(fov_x / 2)` in the world whatever the depth. The front end uses the game's own tan 0.5143 so that the effects keep their size against Kratos in our camera (`SIZE_UNIT` is 1.0). The earth stomp now reads as a ring of dust and debris with a cloud over it.
- **The blades' flames** (`gomaiblade` of `R_WEAPON0_5`, `kratos-play` follows each blade with it). The effect's emitters sit in the blade model's raw space; the rig joint carries the 1/64 scale (an effect rig that only animates emitter rates has no transform track: its joints now stay in the bind pose), velocities keep the joint's rest scale, the effect repeats its clip while it runs on. Two groups of emitters, started separately: the cyan discs `FXC_EG*` (gouraud fans from the centre colour (119, 222, 255) to a cyan rim, `MAT_pticleMat` tint 2.0) run all the time, as in the retail idle frame; the yellow flames `FXC_BD*` run while an attack move runs (the reference videos show yellow fire in slashes and no fire in the idle frame; the data's own flames are orange and yellow, never cyan; MEDIUM that the game gates them by the attack).
- **Disc shapes** (render routine 4) are real fans now: eight rim points in the rim colour (`t[1]`), no texture. Particle materials take the MAT tint like the models.
- **What still differs from the retail frame:** the blade and the armour are lighter and more yellow in ours. The game adds the level's light to the baked vertex colour (`colour = ambient + tint * vertex colour`, section 1.1 of `rendering.md`); the Rhodes opening's ambient is blue (`PSambLite1`: 0.86, 0.97, 1.0), which makes the retail blade silver and its glow cyan-blue. The per-object light is not ported.