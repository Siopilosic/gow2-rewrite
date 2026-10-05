# Environment recreation: starting audit (2026-10-02)

Request: a 1:1 visual recreation of every God of War II environment (geometry, textures, lighting,
fog, sky, particles, animated scenery), with no gameplay.

## Inputs reviewed

| Input | Finding |
|---|---|
| `scripts/` (39 Python files) | **Not God of War II tools.** They target *The Warriors* (Rockstar, RenderWare). Every loader asserts the ELF hash of `SLUS_212.15` and reads `WARRIORS.WAD` / `WARRIORS.DIR`. They decode RenderWare atomics, world-sector plugins 0x3f0/0x3f1 and TXD dictionaries. They expect a `research/scripts` layout. None of them can read GoW II data. Reusable ideas only: GS texel addressing and VIF/DMA packet walking (`native_geometry_probe.py`, `texture_slice.py`) |
| `GOW2 Extract/` | Layer-0 dump: `SCUS_974.81`, `GODOFWAR.TOC` and `SYSTEM.CNF` are byte-identical to `extracted/`. `PART1.PAK` is present, `PART2.PAK` (layer 1) is **missing**. It adds nothing over `extracted/pak/`, which already holds all 292 WADs from both layers |
| This project | see below |

## Level inventory (disc)

There are 122 level WADs in 10 groups (`R_*` resource WADs excluded):

| Group | WADs | Group | WADs |
|---|---|---|---|
| RHOD | 13 | PAL | 20 |
| ISLE | 16 | BOG | 19 |
| ATLAS | 7 | SPIR | 10 |
| PEGA | 12 | CMBT | 21 (challenge arenas) |
| FREE | 3 | ZEUS | 1 |

The mapping of WAD groups to story areas is not established from evidence yet. `RHOD10` = Rhodes was observed at runtime (`runtime-validation.md`); the others are unverified.

**Visual record types in level WADs**

| Type | Records | MB | Meaning |
|---|---|---|---|
| `MDL_` | 21,515 | 223 | models (renModelServer 0x0f) |
| `GFX_` / `PAL_` | 6,547 / 6,476 | 111 / 4.6 | images / palettes |
| `TXR_` | 9,721 | 0.9 | textures (GFX + PAL) |
| `MAT_` | 41,080 | 1.3 | materials |
| `ANM_` | 8,509 | 48 | animation |
| `PTC_` / `FXC_` | 7,750 / 9,603 | | particles / effects |
| `ENV_` | 3,060 | | env records |
| `CAM_` | 6,510 | | cameras |
| `PSd…` | 2,496 | | lights |
| `CDV_` / `VSV_` | 2,308 / 2,424 | | collision / visibility |
| `go*` | many | | scene nodes |

## Present / incomplete / missing (for visual recreation)

| Area | Status | Evidence |
|---|---|---|
| Disc, TOC, WAD reading | **Present** (verified) | `formats.md`, `gow2-rs` |
| GFX/PAL image decode (4/8-bit, CSM1 palette swizzle) | **Present** (verified visually) | `gow2 textures` |
| TXR → GFX/PAL linking | **Present** | C-A7 |
| Scene-node layout (transforms at +0x20, hierarchy, contexts) | **Partial**: runtime-confirmed for nodes in RAM; the on-disc `go*` record → transform path is partly known (0x48 bytes at `+0x20`) | C-D10, H-O4 |
| `MAT_` materials | **Missing**: format undecoded; construction deferred to GroupEnd (C-D7) | |
| **`MDL_` geometry** | **Missing**: no decoder. The largest asset class (223 MB), probably VIF/VU1 packet data | |
| VU1 microcode (lighting, fog, transforms) | **Missing**: needed for PS2-accurate shading | |
| Lights (`PSdirLite`, `LGT_`), fog, sky, `ENV_` | **Missing** | |
| Animated scenery (`ANM_`), particles (`PTC_`/`FXC_`/`PRT_`), shadows (`SHG_`) | **Missing** | |
| Collision (`CDV_`/`COL_`) | **Missing** | |
| A renderer / viewer / level project | **Missing** | |
| Reference captures per area (to verify "1:1") | **Missing**: only three RAM captures so far | |

## What 1:1 requires, in order

1. **MDL_ decode.** Find renModelServer's model loader and VIF packet layout; output vertices, UVs, colours and material ids. Verify against PCSX2 (GS dump / screenshot of a known model).
2. **MAT_ decode** (texture binding, blend and alpha modes) and TXR/GFX linkage per material.
3. **Placement**: `go*` instance transforms and hierarchy, per level and per WAD group (sections stream in and out).
4. **Shading**: VU1 microprograms plus the light, fog and `ENV_` records, to reproduce PS2 lighting exactly rather than approximate it.
5. **Animated scenery** (`ANM_`), particles/FX and sky.
6. **Viewer** (Rust): level selection and section states.
7. **Verification**: per area, side-by-side frame captures from PCSX2 at fixed camera positions. "Nothing missing" can only be claimed against such captures.

Nothing of steps 1–7 exists yet. This document will be updated as each step is evidenced.

**Progress.** Step 1 started (`models.md`):
- Done: the record handler and the blob hierarchy, confirmed from code.
- Done: the DMA/VIF packet layout of model parts, decoded (`tools/mdl_probe.py`).
- Next: vertex decode and checks against the VU1 microprogram and PCSX2.
- **Vertex decode works:**
  - positions, normals, vertex colours, strips and the 1/16 scale are HIGH;
  - the Kratos model and all 60 RHOD10 models export;
  - RHOD10 assembles into a coherent scene (`models.md`).
- **Textures work:**
  - UV /4096, part → material slot → MAT → TXR → GFX/PAL;
  - PSMT8 unswizzle for 8-bpp enc-0 textures, a correction of the old texture claim (R13);
  - textured fish pillar from RHOD10.
- **Open:** material blend/alpha fields, node transforms for instanced models, sky handling, VU1 lighting/fog, animation, particles, and a real viewer.

**Progress, 2026-10-02 (later).**
- **Placement resolved.** Ref instances plus the MDL record offset and scale (`models.md`).
- **All 122 level WADs export** to glTF with `tools/gltf_export.py` → `analysis/levels/<WAD>_gltf/`, 0 errors. Fixes on the way:
  - `:` in material names breaks file paths;
  - sub-block 8-bpp textures.
  - SPIR37 and SPIR38 have no geometry (scripts/contexts only: transition stubs?).
- **Blend modes decoded.** MAT `+0x38` holds the GS ALPHA selector (normal/additive/subtractive, HIGH) and a textured bit (CONFIRMED by data) (`models.md`).
- **Viewer.** `analysis/levels/viewer.html` lists every level (`levels.json`, `?level=NAME`).
- **Spot checks render coherent scenes:** RHOD10, BOG10, ISLE20, PAL15.
- **Corrected:** R14, the `+0x50` sky flag; R15, the MDL `+0x48` scale divides (all levels re-exported, RHOD10 vista now coherent).
- **Still open:**
  - VU1 lighting/fog. Started in `rendering.md`:
    - VU programs extracted;
    - model renderer = program B (CONFIRMED by savestate);
    - light records decoded;
    - fog identified as a 3-pass Z-buffer post-process driven by `SCR_Fog` scripts; exact curve decoded (`rendering.md` §3.2), the viewer matches it;
    - **scenery is unlit at run time** (shader variant 0x31/0x30 on 43k of 49k parts): vertex colour × texture is the game's formula. Dynamic lights affect only characters and a few props (`rendering.md` §1.1).
  - MAT tint and other fields;
  - the `+0x50` meaning;
  - flash-volume toggles (`FlashCube*`, scale 128, hidden by name in the viewer);
  - ~~`ANM_`~~ transform animation exported (rigs, joint binding, clip 0; `animation.md`); ~~material tracks~~ (done: type 3 colour, type 8 UV scroll, played in the viewer); ~~`go` subtype-3 placement~~ (done for rigs, `animation.md`); ~~particles~~ (see below);
  - placement: rigs placed by their `go` node, and ref instances corrected to the engine's row-vector rotation (`models.md`, 2026-10-02); all levels re-exported;
  - particles (`particles.md`): PTC_ shape records and VU1 program A decoded as a per-particle op VM (spawn-state copy, random ranges, linear/ballistic/colour/keyframe ops over age, lifetime confirmed); emitter spawn path traced (rate, surface sampler). Render routines and MAT binding decoded (§3.3). Emitter fields anchored by code (§4). Bursts come from type-10 ANM tracks that animate the rate; GS blend and depth write decoded from shape flags (§3.4); viewer playback prototype (§6, CPU op VM). Rig emitters now sit at their objects. Open: attach joint, size scale;
  - WAD-group → story-area map;
  - PCSX2 reference captures.
