# ANM_ animation records (AnimServer, id 3)

_Started 2026-10-02 for the environment track (`environment-audit.md` step 5, animated scenery)._

## Container layout. HIGH (code: `FUN_00117c18`, `FUN_00117cb8`, `FUN_001d3678`; disc survey)

| Offset | Type | Meaning |
|---|---|---|
| +0x00 | u32 | 3 (server id) |
| +0x08 | u32/float | varies (e.g. `0x00300000`, or a float such as 0.22 / 532 on material and FX anims). UNKNOWN |
| +0x0c | u32 | record size |
| +0x10 | u16 | 1 |
| +0x12 | u16 | **group count** |
| +0x18 | u32[n] | **group offsets** (relative to the record) |
| +0x1c | u32 | low byte = record kind (0, 8, 10, 3, 4, 12, 11, 0x40, 0x80); with one group this word sits inside the offset table area. MEDIUM |

**Group** (at record + offset):

| Offset | Meaning |
|---|---|
| +0x08 | flags. `0x40` (7,668 groups) = the channels carry name hashes and are searched by hash (`FUN_00117c18`; with param 3 it tests `0x20` instead) |
| +0x0c | **channel count** |
| +0x14 | group name (e.g. `default`) |
| +0x2c | hash of the group name (`FUN_00181428`; `FUN_00117cb8` looks a group up by name) |
| +0x34 | u32[n] channel offsets (relative to the group) |

**Correction (2026-10-02, same day):** the per-group entries are **clips**, not channels. `FUN_00100728(anm, clipIndex)` plays one; `FUN_00117c18` finds a clip index by name hash.

**Track descriptors** follow the group offset table: `+0x10` u16 = track count, then at `+0x18 + 4·groups` 8 bytes each: `u16 type, u8 b2, u8 subtracks, u32`. CONFIRMED (`FUN_0027c2e0`, factory `FUN_00100e18`). The factory creates one **player** per track by type, and the player owns `subtracks` consecutive clip blocks:

| Track type | Count on disc | Player vtable | Use (from names) |
|---|---|---|---|
| 0 | 24,971 | `0x2f5d20` | node/joint transforms (doors, lamps, rocks, skeletons) |
| 10 | 8,188 | `0x2f5cb0`/`0x2f57e0` | ~~rock/debris objects (`ANM_rockfloor`)~~ **emitter parameters**: one block of f32 keys per emitter, slot·4 = byte offset into the emitter's parameter block, slot 8 = rate (`particles.md` §4). HIGH (2026-10-02) |
| 8 | 2,215 | `0x2f5bd0`/`0x2f5b60` | materials (`ANM_phong17`, `ANM_bloodDrips`): **UV offset** (slot 0 u, slot 1 v), f32 keys. HIGH (section "Material tracks") |
| 3 | 1,597 | `0x2f5cb0`/`0x2f5c40` | material colour (`ANM_lambert2`) and others: **RGB multiplier** (slots 0–2), f32 keys. HIGH (section "Material tracks") |
| 1, 2, 4–7, 9, 11–15 | ≤ 1,905 each | various | hero/camera specials |

**Clip** (at group + offset):

| Offset | Meaning |
|---|---|
| +0x00 | flags; `& 0xf000` = codec selector in the sampler, `& 0x108` = additive/override (MEDIUM) |
| +0x08 | 3 |
| +0x0c | f32 1.0 (rate) |
| +0x14 | f32 **duration** in seconds (`ANM_swinginlamp` 10.2) |
| +0x1c | s16 next clip index (−1 = none) |
| +0x20 / +0x24 | name hash / name (`default`) |
| +0x30.. | three vec4 (base pose / bounds; `ANM_firstdoor` (465, 1137, −772)) |
| +0x40 | u32 count of child anims triggered on start (`FUN_00118788`) |
| +0x60 + 16·k | **clip block** per sub-track k: `u16 ?, u16 segCount, u16 ?, u16 ?, u32 segTable (clip-relative), f32 frameTime (1/30)` |

**Segment** (12 bytes, `FUN_00112380`): `u16 channelSlot, u16 flags, u16 keyCount, u16 startFrame, u16 extra(14 bits) | offHi(2 bits), u16 offLo`; byte 3 adds `·0x10000`. The data at the offset holds a sub-segment list, a shift table and **int8 deltas** that are integrated into 2¹⁴ fixed-point accumulators per frame (the same scheme as `FUN_00119bb0`).

- `ANM_firstdoor`: all three blocks have `segCount = 0`, so it is a static pose.
- `ANM_swinginlamp`: block 0 has two 224-key segments of int8 deltas forming a damped oscillation (the swing); block 2 has an 82-key u16 stream around `0x0800`.

**Call chain** (all CONFIRMED by code): player `+0x5c` = `FUN_00106380` (play clip) → layer `FUN_0027de08`/`FUN_0010b0f0` (vtable `0x2f5d98`) → clip instance `FUN_00112060` (vtable `0x2f5dc8`) → **sampler `FUN_00112380` (17.6 KB)**, helpers `FUN_00272430`/`FUN_00272730`/`FUN_00272b28`.

## Clip flag survey (7,361 ANM records)

_Originally titled "Channel types". These are the clip `+0x00` flag words; see the correction above. Kept as surveyed._

| Type (low byte) | Count | Examples | Reading |
|---|---|---|---|
| 0x42 (+ flags 0x1000–0x7000, 0x100) | ~7,700 | `ANM_rockfloor`, `ANM_joint11`, `ANM_firstdoor`, `ANM_eastcap` | node/joint transform track (positions such as (465, 1140, −772) in `ANM_firstdoor`). MEDIUM |
| 0x41 (+ flags) | ~4,300 | `ANM_phong17`, `ANM_lambert2`, `ANM_atlaseyes`, `ANM_flag1` | material track (named after the MAT; `FUN_001d3678` binds `$`-prefixed names to `MAT_`). MEDIUM |
| 0x21 / 0x22 (+ 0x3000, 0x43000, 0x800000) | ~800 | `ANM_hero`, `ANM_barbking`, `ANM_archer00` | skeletal character clips |
| 0x74 | 1,024 | `ANM_hero` | UNKNOWN |

- Key spacing 1/30 s (`0x3d088889`) is stored in the track headers.
- `ANM_lambert2` holds an RGB float ramp (0.197 → 0.368 → 0.0246, triplets): a material colour animation.

## Evaluator

- `FUN_00119bb0` (9 KB, AnimServer vtable) integrates **delta-compressed** channels: bit masks select components, and int8 deltas are scaled by per-channel shifts into 2¹⁴ fixed-point accumulators, with linear blending between keys. This looks like the skeletal codec (0x21/0x22). Not decoded.
- ~~The decoder for 0x41/0x42 (float keys) is not located yet.~~ Found: `FUN_00112380` (above). It uses the same delta scheme, not float keys.

## Segment codec, decoded. HIGH (code + data), 2026-10-02

`tools/anm_decode.py <wad> <ANM_name>` implements it and prints per-slot curves.

- **Blocks per transform track (type 0).** Clip block 0 (`+0x60`) holds rotation and writes the int accumulators at instance `+0x5c`. Block 1 (`+0x70`) holds translation: floats at `+0x60`, via `FUN_00272730` with scale 1/256 or float keys via `FUN_00272b28`. Block 2 (`+0x80`) holds scale: floats at `+0x64`, via `FUN_00272430` × 1/2048.
- **Slots.** `slot = joint·4 + component`. A mask bit k selects component k (the `plzcw` idiom); each further mask row adds 16 slots.
- **Segment data.**
  - `keyCount = 0`: a run list. `data[0]` int8-delta runs come first and `data[1]` is the total; the remaining runs are int16 absolute keys. Each run is `count, startFrame, extra|offHi<<14, offLo`.
  - `keyCount > 0`: one flat key list. `flags & 1` selects int8 deltas, otherwise int16 absolute keys.
  - **Shift table** (`flags & 2`): `rows, stride, u16 base, u16 mask[rows], s8 shift[stride]`. Without it, the runtime default at `0x2fd090` (`01 01 0000 0001`) applies and the shift is `(s8)flags >> 4`.
  - **Integration:** absolute keys set `acc = v << 14`; each frame of a delta run adds `d << (14 − shift)`. Linear interpolation runs between frames.
- **Rotation is a quaternion.** `FUN_0010ab90` takes the four components × 2π/16384 (`0.0003834952`), normalises them with `rsqrt`, and builds a matrix with the 2·xy… terms. So the scale cancels out: x, y, z, w are int units with w ≈ 16384 for identity.
- **Validation on `ANM_swinginlamp`.** The rig record has 7 joints (`swinginLamp`, `lamp01Anim`, `torchGlow04`, `pPlane640`, `lamp02Anim`, `torchGlow05`, `pPlane641`).
  - Rotation slots 6 and 18 are the z component of joints 1 and 4: the two lamps swing in a damped oscillation, peak about 17°, 6 frames apart, over 10 s.
  - Scale slots 12–14 and 24–26 are joints 3 and 6 (the glow planes), flickering at 0.75–1.18 (2048 = 1).
- **Survey.** Monotonic rotation runs of exactly ±16384 recur (`ANM_crankturntable`, `ANM_birdwalking`, `ANM_group4643`); a quaternion component reaching 1.0 means a 180° turn.

## Rigs, joint binding and glTF export. HIGH, 2026-10-02 (`tools/rig_anim.py`, `tools/gltf_export.py`)

- **Rig record.** It is the unnamed tag-1 record inside a `go<name>` group, with header `u16 1, u16 1`, followed by refs to `ANM_`, `MDL_` and `ESC_`.
  - `+0x04` = joint count nj.
  - Joint entries are 16 bytes at `+0x18`: `s16 firstChild, nextSibling, parent, ?`.
  - Names follow. With X = align16(0x18 + 40·nj), the local matrices start at X + 0x30: row-vector, 64 bytes each, translation in row 3. Translations sit at M + 64·nj + 0x20 and scales 32·nj after that.
  - Checked on rigs with 1, 2, 7, 9 and 157 joints.
- **Root joint = model transform.** The root's translation is the MDL record `+0x38` offset and its rows are 1/scale (`+0x48`). Several animated rigs (`lamps`, `firstRoom`, `godRays`) also carry world positions that their MDL record lacks.
- **Vertex binding.**
  - Each part has a joint palette of `u16(C+10)` entries: s32 values after its DMA entry table.
  - Each GIF tag in a batch header covers NLOOP vertices, and its w word selects the palette slot `(w & 0x3ff)/4`. VU program B loads that matrix from base+16+w.
  - Vertices are **joint-local**. `MDL_swinginLamp`: 485/484 vertices on the two lamp joints, 4/4 on the glow planes.
- **Export.**
  - A model with a multi-joint rig or an `ANM_` becomes a glTF skin: weight 1 on the palette joint, identity inverse binds, and joint nodes with TRS taken from the rig matrices under the instance node.
  - Clip 0 becomes a glTF animation (LINEAR, dt = 1/30). Rotation components missing from a clip come from the bind quaternion × 16384 and the result is normalised.
  - RHOD10: 25 rigged models, 13 animations.
- **Viewer.** It plays clips with an `AnimationMixer`. Clips named break/chunk/debris/smash are gated behind "one-shot clips", because in game they are script-triggered (a viewer heuristic).
- **Open (MEDIUM).**
  - Quaternion handedness: the transpose convention between GoW row-vector matrices and glTF is assumed.
  - Clips other than clip 0.
  - Non-transform track types (material 8/3, debris 10).
  - ~~The world placement of `go` nodes of subtype 3 (e.g. `goswinginlamp`), which the exporter does not parse yet.~~ Done (section "go node placement").

## Material tracks (types 3 and 8). HIGH, 2026-10-02

- **Binding (HIGH, WAD structure):** a material's `ANM_` sits right after it in its group: `GroupStart, MAT_x (body), ANM_x (body), MAT_x (ref)`. Example: `MAT_skyBackDrop` → `ANM_skyBackDrop`. The engine side (`FUN_001d3678` binds `$` names to `MAT_`) is not traced further.
- **Codec:** f32 absolute keys, the same segment layout as transform translation; `tools/anm_decode.py` decoded these blocks wrongly before (block index mod 3).
- **Type 3** (`b2` = 1): **RGB colour multiplier**, slots 0–2 = r, g, b. HIGH from the shape of the data:
  - `ANM_athHouseLampGlowing` flickers between 0.64 and 1.30 over a 1.67 s loop;
  - `ANM_blockGlowing` pulses each channel separately (0.87/0.82/0.76 → 1.12/1.05/0.97);
  - `ANM_lambert2` ramps 0 → 0.37 → 0 in 1 s.
- **Type 8** (`b2` = 128): **UV offset**, slot 0 = u, slot 1 = v. HIGH for being a scroll; MEDIUM for the sign. Every curve is a linear ramp of whole texture repeats per loop:
  - `ANM_arrows` u 0 → −1 in 3.67 s, `ANM_flagTrick` u 0 → −1 in 1.67 s;
  - `ANM_hugeFire02a` u 0 → 2 and v 1 → 0 in 6.67 s (the v curve eases);
  - `ANM_skyBackDrop` u 0 → 1 and v 1 → 3 in 400 s.
- RHOD10 has 43 animated materials: water scrolls and falls, crashing waves, sky layers, clouds, fires, arrows, glowing windows, lamps, explosions.
- **Export and viewer:** `tools/gltf_export.py` writes them to material `extras.anim` (`color` / `uv` key lists, `dt`, `duration`). The viewer multiplies the material colour, and sets the texture offset on its own copy of the texture. Both loop with the clip.
- Still open: the in-game sign and units of the UV offset (whether the GS adds it to UV or subtracts it), and the `+0x08` float per ANM (0.22, 1.0 …).

## go node placement (subtype 3). HIGH for rigs, 2026-10-02

The record that names a rig group (`gochunks2`, `gobalcony`, `gofirstroom`) is a 104-byte tag-1 record with header `u16 1, u16 3`. RHOD10, ZEUS10, SPIR50 and BOG20 have 640 of them, all 104 bytes; 164 have a rotation.

| Offset | Meaning |
|---|---|
| +0x1c | u32 (0x5c, 0x7e …). UNKNOWN |
| +0x20 | 3×3 rotation, 9 floats, rows |
| +0x44 | translation vec3 |
| +0x50 | vec3 **world bounds centre** (+0x5c = 0) |
| +0x64 | −1 |

- **Rigged models:** world = rig-local point · R + W (row vectors). Evidence:
  - The +0x50 centre lies within about 25 units of the transformed root joint for most RHOD10 rigs: DoorSparkle 4, sparkle 4, chunks2 20, balcony 24, LampCorridor 24, the breakable baskets and bags 0–5.
  - Rotated `gofirstroom` (33.8° about Y) lands 104 units from the centre in row-vector order, but 6711 units off with the transpose.
  - The ingame1 RAM dump holds a copy of the record at 0x19bcd00 (R, W and the centre).
  - In the viewer the rotated `firstRoom` now joins the round tower; at its old placement it floated beside it.
- **Unrigged models** in go groups (`ColossusSheet`, `staticWindows`, `moonLightPool`) already sit at world coordinates through their MDL record offset. Adding the go transform would put ColossusSheet 6971 units off, so the exporter keeps the model-record placement for them. Why the engine doesn't double-apply it is not traced. MEDIUM.
- `tools/gltf_export.py` places 25 RHOD10 rigs this way (node extras `placement: go`). Rig-attached particles (balcony, chunks, breakWallAA1, DoorSparkle) now appear at their objects.
- ~~Open:~~
  - ~~`fallingDebris` (about 2700 units from its centre) and `lamps` / `grandHall` / `godRays` (264–639) still differ from their bounds centre. Their geometry spreads widely or starts displaced, so the centre may be the wrong comparison for them.~~ Rechecked the same day with the **full skinned geometry** in world space (bind pose) instead of the root joint. The centre of its bounds box matches +0x50 for chunks2 (2 units), swinginLamp (9), balcony (5), lamps (1), grandHall (16) and godRays (0), so placement is right for them. HIGH.
  - Left over: `firstRoom` is 153 units off (geometry about 780 across; the bounds may not be a box centre), and `fallingDebris` is 2740 off. Its clip (5.77 s, 152 animated joints) moves pieces by up to −5500 units on z, presumably parking spent debris out of view, so its bounds probably cover the animation. MEDIUM, open.
  - The ref-instance convention in the exporter (`R` applied as columns) was not re-checked against this finding.

## Runtime controller

The per-character animation controller (`FUN_0023ad28`, state table, distance-driven playback and the blend node types of `FUN_00108d08`) is documented in `docs/character-update.md` §7 (2026-10-03).

## Next
1. ~~Locate the sampler~~ (done: `FUN_00112380`).
2. ~~Decode the segment codec~~ (done above). Open: the bind-pose source for components that are not animated, and the exact translation path (1/256 vs float).
3. ~~Export~~ (done above).
4. ~~Placement of `go` nodes of subtype 3~~ (done for rigs, see above), ~~material tracks (types 8/3)~~ (done, see above), the remaining clips, and a script-trigger map for one-shot clips.

## Kratos: skinned character export (2026-10-03)

- **Rig.** `gohero`'s rig record (R_HERO00, 27,904 bytes) has **123 joints**: `main`, `template.0`,
  `pelvis`, `vertebrae2..4`, `neck`, `head`, `jaw`, 22 face joints (`JOL*`/`JOR*` eye, brow, lip), both
  arms (`Clavicle`, `Humerus`, `Radius`, `Wrist`, index/ring/thumb finger joints), weapon attach joints
  `lWeapIH`/`rWeapIH` (in hand), ~~`lWeapOH`/`rWeapOH` (on the back)~~ `lWeapOH`/`rWeapOH` (the free blade joint the clips animate; see "Blade attachment" below), `lChain`/`rChain`/`lChainW`/`rChainW`,
  `rightBladeBack`/`leftBladeBack`, a 42-joint skirt chain (`skirtJoints`, `joint0..41`; cloth, `SCR_cloth`),
  legs (`Femur`, `Tibia`, `Metatarsal`, `Phalanges`), and `zeroJoint`, `synchJoint`, `linkJoint` (sync
  points for grabs). CONFIRMED (disc; parent links valid for all 123).
- **Correction to "Rig record" above:** the names are 24-byte strings starting at `0x14 + 16·nj`
  (4 bytes before the end of the joint entries), not at `0x18 + 16·nj`. Joint entries at `+0x18` are right.
  `tools/rig_anim.py rig_joint_names`.
- **Character vertices are in bind-pose model space**, not joint-local as for the scenery rigs: the static
  OBJ (`analysis/models/hero_0.png`) is a correct Kratos, and the skin only renders correctly with real
  inverse bind matrices (inverse of each joint's world bind matrix, W = L · W_parent). HIGH (render check).
  `tools/gltf_export.py --bind-space`.
- **Clips.** `ANM_hero` has 189 groups; 355 entries carry a name at clip `+0x24` (hash at `+0x20`, equal
  to `gow_hash(name)` for 193). Names include `attComboSlash01..05`, `attBrutalSlash`, `attAirSlashH1..3`,
  `attSpecial*`, idles. 143 of the 891 hero move animations (`MOV+0x08`) are found in this WAD; the others
  are in the other `R_HERO*` WADs. The segment codec of this document decodes them (slot = joint·4 + comp;
  pelvis = slots 8–11).
- **Quaternion convention:** comparing frame 0 with the bind rotation, the raw components match better
  than the conjugate (mean |dot| 0.94 vs 0.86 over 16 joints of `attComboSlash01`; `lHumerus` 0.99 vs 0.50).
  MEDIUM.
- ~~**Open (why the exported clips still look wrong):** most joints get only some of their four quaternion
  components from the segments; the exporter fills the rest from the bind pose, which mixes two
  rotations. The clip must carry its own constant values (a base pose or constant-channel block) that are
  not decoded yet.~~ Wrong diagnosis (2026-10-03, same day): filling the w component from the bind pose
  or from `√(1 − x² − y² − z²)` gives the same result. The real faults were in the segment decoding:
- **Codec corrections (2026-10-03), HIGH.** Test: a quaternion's length is constant within a clip
  (20,860 int units for Kratos), so every sample with all four components can be checked.
  1. **One accumulator per slot across all segments of a block** (the game keeps one at clip instance
     `+0x5c`). Kratos's clips split one channel into a key segment (2 absolute keys at frame 0), a delta
     segment (frames 1…n) and an end-key segment; the tool had reset the accumulator per segment.
  2. **Segments are applied in time order** (by first frame, absolute keys before deltas;
     `anm_decode.segment_order`), not in table order.
  3. **A delta key or delta run starting at frame `s` produces frames `s+1 …`**: it continues from the key
     already stored at `s`. Evidence: the sum of a looping clip's deltas returns exactly to the frame-0 key
     (e.g. slot 136: key 12,226 + Σ deltas = −1,790 against key −1,791).
  Result over 101 hero clips (`att*`, `nav*`, `wpn*`, `mag*`): 30,847 four-component samples, 149 off
  the expected length (0.5 %), against 242 of 505 in `attComboSlash01` before. The scenery clips shift by
  one frame with fix 3. The exported Kratos (`--bind-space --anm ANM_hero --clips …`) plays recognisable
  attacks in the viewer (`analysis/levels/R_HERO00_gltf`, `viewer.html?…&clip=attComboSlash01`).
- **Quaternion convention:** raw components (no conjugate), confirmed by the length test and the render.

### Kratos's blades (2026-10-03)

- The blade model is `MDL_MAIblade` (634 vertices, 414 triangles, materials `MAT_blade3a`/`MAT_blade3b`) in
  the `R_WEAPON<n>_<stage>` WADs (stages 0–5, the upgrade levels; every `_5` WAD checked holds the same
  mesh). Each WAD also has the blade's `gomaiblade` rig (one joint, scale 1/64: the MDL `+0x48`
  quantisation), `ANM_maiblade`, the `CDV_gomaiblade` collision (`docs/combat.md` §2.1) and fire effects
  (`MDL_FXfirePath`, `gofxlcombo3f`/`gofxrcombo3f`, `MDL_combo3fExplode`). At 1/64 the blade is 16.75 units
  (≈ 1.05 m) long.
- `tools/gltf_attach.py` instances a prop mesh under named joints of a character export, carrying the
  prop's one-joint transform. With the blades on `lWeapIH`/`rWeapIH` the exported Kratos
  (`analysis/levels/KRATOS_gltf`) swings both blades through his attacks.
~~- MEDIUM: the in-hand attachment matches the look of the attacks, but the game also moves the blades out~~
~~  on the chains (`lChain`/`rChain`, `lChainW`/`rChainW`, the `+0xf2` attach joint of~~
~~  `docs/character-update.md` §1); which joint holds the blade at each moment is set by code not read yet.~~
~~- **Attachment mode per move** (read the same day). `tActionForceAttachment` (140 hero actions) runs~~
~~  `Scr_ForceAttachment` (`0x001bbf30`): it walks the character's two weapon attachments (virtual `+0x74`)~~
~~  and, when the attachment's type byte `+0x20` matches the action's item byte `+0x14` (item 1 ↔ type 0,~~
~~  item 2 ↔ type 1), sets the attachment's mode `+0x12` to the action's byte `+0x15` (0, 1 or 2) and marks~~
~~  it dirty (`+0x18 |= 4`). HIGH (code). In the hero data all 52 blade combat moves (`MOV_CombatIdle`,~~
~~  `MOV_BasicSquare01`, `MOV_CombatRun` …) force item 1 to mode 1, and all 88 sub-weapon moves~~
~~  (`MOV_BoneSquare01` …) force item 2 to mode 0; navigation moves (`MOV_Walk`, `MOV_Jump`, climbing) carry~~
~~  none, so they keep the default. MEDIUM reading: mode 1 = in the hands (`WeapIH`), the default = stowed on~~
~~  the back (`WeapOH`), as Kratos carries the blades on his back out of combat; mode 2 and the default value~~
~~  are not confirmed. `tools/gltf_attach.py` attaches to the hand joints only.~~

Superseded by "Blade attachment" below (2026-10-03): `WeapOH` is not the back joint, and the action's item 1
is the hand-held sub-weapon, not the blades.

### Blade attachment: back, hand and chain (2026-10-03)

**Attachment records** (`DC_WAD_R_Hero`, CONFIRMED disc). `ATT_Chains` lists two `tChained` records
(`tChained_101` left, `tChained_102` right); `ATT_Bone`, `ATT_Hammer`, `ATT_Medusa`, `ATT_Olympus` and
`ATT_WindBow` each list one `tHand` record. Layout (self-relative string pointers):

| Offset | Field | Left blade | Medusa head |
|---|---|---|---|
| +0x00 | object name | `goMAIBlade` | `goMedusaHead` |
| +0x04 | slot 1 joint (hand) | `LWeapIH` | `RWeapIH` |
| +0x08 | slot 2 joint (free) | `LWeapOH` | `RWeapOH` |
| +0x0c | slot 0 joint (stowed) | `LeftBladeBack` | empty |
| +0x10 | snap distance (m) | 0.35 | 2.0 (Wind bow 1.0, Bone 0) |
| +0x15 | byte (8 = no world ray) | 8 | 8 |
| +0x18, +0x1c | world-ray extent along the object (Bone 2, 2; blades 0) | 0, 0 | 0, 0 |
| +0x20 | byte: 1 = chained, 0 = hand | 1 | 0 |
| +0x21 | bit 0: the object has its own animation (sub-weapons) | 0 | 1 |
| +0x24, +0x28 | (tChained only) chain joints | `LChain`, `LChainW` | — |

**Attachment object** (`FUN_00245b48` base constructor, `FUN_0023cf28` chained constructor). `+0x00`
record, `+0x04` owner, `+0x08` the object instance (the blade), `+0x0c/+0x0e/+0x10` the joint indices of
slots 0/1/2, `+0x12` the **mode** (= slot, 2 at creation), `+0x14` index (0 left, 1 right), `+0x18` flags,
`+0x40` vtable (`0x2f1378` base, `0x2f0e80` chained). Chained: `+0x110/+0x112/+0x114` joints `LChain`,
`LChainW`, `Pelvis`; `+0x118/+0x11c` follow weights; `+0x128/+0x12c` blade trail; `+0x138`/`+0x140`/`+0x150`
the chain-link, chain-glow and stencil meshes. RAM `ingame2` (Kratos walking): left object `0x7f23a0` joints
64/46/118 = `leftBladeBack`/`lWeapIH`/`lWeapOH`, right `0x7f2500` 63/61/116, both mode 0, vtable `0x2f0e80`
(update `FUN_0023e950`); chain joints 47/119/2 = `lChain`/`lChainW`/`pelvis`. CONFIRMED.

**Mode choice, every frame** (`FUN_00245f78`, called from the update). HIGH (code), CONFIRMED (RAM + clips).
With `J0`, `J1`, `J2` the world positions of the three slot joints and `r` = record `+0x10` × 16:

- `d1 = |J2 − J1|` (free joint to hand), `d0 = |J2 − J0|` (free joint to back);
- if `d1 < d0`: mode 1 (hand) when `d1 < r`, else mode 2;
- otherwise: mode 0 (back) when `d0 < r`, else mode 2;
- the choice is skipped (the mode is kept) while the attachment's dirty flag `+0x18 & 4` is set (cleared
  here, set by `Scr_ForceAttachment`) or while the owner's current move has flag `+0x04 & 2`
  (`character+0x1c8 → +0xb0 → +0x6c`).

So **the clips carry the blade**: each clip animates the free joint `WeapOH`, and the code snaps the blade
to the back or the hand joint when the free joint comes within 0.35 m of it. Evidence: in `navIdle` the
free joint sits 0.02 m from `leftBladeBack` (mode 0, blades on the back, matching RAM), in
`navCombatIdle` it stays 0.6–0.9 m from both (mode 2), and in `attComboSlash01` at 0.3 s the left free joint
is 4.8 m from the hand (the blade thrown on the chain). When the mode changes, the update re-reads the
three joints from the strongest animation layer alone (the blend layer with the largest weight product),
so a cross-fade does not flicker between slots.

**What each mode does** (`FUN_0023e950`, chained update):

| Mode | Follow weights `+0x118/+0x11c` | Chain (`+0x18 & 0x10000`) | Blade trail (`+0x18 & 0x10`) |
|---|---|---|---|
| 0 back | 1, 1 (snap) | hidden (`FUN_00241348`) | off |
| 1 hand | 1, 1 (snap) | shown (`FUN_002411f0`) | on |
| 2 free | `GBL+0x90` = 0.4 (lag) | shown | on |

The blade's matrix is blended each frame from its previous matrix towards the slot joint: rotation by
`weight × dt × 60`, position by the second weight, so in mode 2 the blade trails the free joint (0.4 of the
gap per 1/60 s) and swings on the chain. The result is written straight into the blade object (parent joint
`+0x60` = −1, as in RAM). HIGH. The chain mesh is built once with `GBL+0x98 / GBL+0x9c` + 1 = 17 / 0.25 + 1
= **69 links** (RAM `+0x14c` = 69, two strips) between `LChain` and the blade, plus a stencil strip
(232 × 3). MEDIUM: the link placement itself is not read. In RAM the flag `0x10000` is set although the
mode is 0; what sets it again after the mode-0 branch is open.

**Forced modes** (`Scr_ForceAttachment`, `tActionForceAttachment`, action `+0x14` item, `+0x15` mode). HIGH
(code), CONFIRMED (data). Item 1 matches attachments with record byte `+0x20` = 0 (the hand-held
sub-weapon), item 2 those with `+0x20` = 1 (the chained blades). The native sets the mode and the dirty flag
every frame while it runs, so it overrides the distance rule for the rest of the move. In the hero data:

- 88 actions force item 2 = the blades to mode 0 (on the back): every sub-weapon move (`MOV_BoneSquare01`,
  `MOV_OlympusSquare01` …) and some navigation moves (`MOV_Stand`, `MOV_Jump`, `MOV_Fall`, `MOV_Walk`,
  `MOV_Land`; several actions per move that differ at `+0x18`, probably one per selected weapon). So **the
  blades go on the back while a sub-weapon is in use**;
- 52 actions (`MOV_CombatIdle`, `MOV_BasicSquare01`, `MOV_CombatRun`, `MOV_Stand`, `MOV_Jump` …) force
  item 1 = the sub-weapon to mode 1 (its hand joint). Blade moves themselves are not forced: the clips
  decide. MEDIUM: why blade moves force the hidden sub-weapon to the hand (several actions per move differ
  in `+0x18`, probably a per-weapon condition) is open.

**Viewer.** `analysis/levels/viewer.html` applies the distance rule to the Kratos export (checkbox "blade
attachment rule"; status shows `back`/`hand`/`chain` per blade) and draws the chain as a line from
`lChain`. `?level=KRATOS&clip=navIdle` shows the blades crossed on the back, `clip=attComboSlash03` throws
the right blade about 3 m out on its chain. Not modelled: forced modes, the mode-2 lag, the 69-link chain mesh.

### Skirt cloth (2026-10-03)

**Data.** HIGH (disc), CONFIRMED (RAM `ingame2`).
- Script object `SCP_skirtJoints` (`R_HERO00`, class `SCR_Cloth`) starts the native `Scr_cloth`
  (`0x00105db0`). It names the cloth parameter record **`CLT_Hero`** (its own `DC_CLT_Hero` container, hash
  `0x08f56a8e` = `gow_hash("CLT_Hero")`) and the joint group `skirtJoints`.
- The native's parameter block (instance `+0x54` + `+0x5c`; RAM `0x9525f0`): `+0x00` 0x102, `+0x04` first
  joint 66 (`joint0`), `+0x08` 7 strands, `+0x0c` 6 particles per strand, `+0x10` the `CLT_` hash, `+0x14`
  7 pinned particles, `+0x18` 13 collision spheres, `+0x1c` flags 8 (the init ORs in 2), `+0x20…` the pinned
  joints 66, 72, 78 … 102 (`joint0`, `joint6` … `joint36`: the top of each strand).
- The rig: the 42 joints `joint0..41` are direct children of `skirtJoints` (parent `vertebrae2`), so each
  joint is one particle. No clip animates them (`navWalkFast` has 0 channels on them): the skirt moves only
  by simulation.
- `CLT_Hero` (`FUN_00101d80` copies it): `+0x00` 2.0 and `+0x04` 4.5 (gravity factors at the hem and at the
  belt), `+0x08` 0.98 (velocity kept per step), `+0x0c` 0.5, `+0x10` 1.0, `+0x14` 0.5, `+0x18` 0.25 (not
  traced), `+0x1c..+0x30` ±500 (a box, ×16 at load), `+0x34` self-relative table of per-strand values,
  `+0x38` the sphere table. RAM matches field for field (cloth object `0x96ed30`: `+0x58` 0.98, `+0x68` 2,
  `+0x6c` 4.5).

**Simulation** (`FUN_00105950` each frame unless `DAT_00363048` is set). HIGH (code).
1. Take the root joint's motion since the last frame (`FUN_001030e8`, `FUN_00102e00`) and copy the pinned
   particles from the animated pose.
2. **Verlet step** (`FUN_00102798`, fixed 1/60 s): for every free particle,
   `p' = p + (p − p_prev) × 0.98 + a / 3600`, with `a` = wind (`FUN_00120c20`/`FUN_00121250`, MEDIUM) plus
   gravity `−78.4 × (4.5 − 2.5 t)` units/s² down, `t` = particle position along the strand (0 at the belt,
   1 at the hem). So the top of the skirt falls faster than the hem (×4.5 against ×2).
3. **Constraints** (`FUN_00104580`): distance links along each strand and to the neighbouring strand, rest
   lengths from the bind pose; two free ends move half the error each, a pinned end does not move. Repeated
   up to 6 times; stop when the summed stretch ≤ 0.1 or changes by < 0.005.
4. **Collision**: flag 8 selects `FUN_00103c30` (cloth triangles against the spheres, MEDIUM), otherwise
   `FUN_001033f0` (each particle pushed out of each sphere). Kratos's 13 spheres (RAM, record 0x60 bytes:
   centre `+0x00`, radius `+0x20`, joint-local offset `+0x40`, joint `+0x50`): 5 on `lFemur` (radius 2.55 →
   1.68 units, along the thigh), 6 on `rFemur` (one more at 9.5 units), 2 on `pelvis` (2.07).
5. Write the particles back to the 42 joints (`FUN_001024a0`).

Open: the rest of `CLT_Hero`, the wind source, the triangle collision, a second `SCR_cloth` in RAM (first
joint 51, another character) and the 11 × 7 = 77 particle count in RAM `+0x08..+0x10` against the 6 × 7
grid of the parameters (MEDIUM: probably extra particles between the joints).

**Viewer.** `analysis/levels/viewer.html` runs this on the Kratos export (checkbox "skirt cloth"): 7 × 6
particles, the RAM spheres, the data constants. Without wind and with sphere push-out instead of the
triangle test. With it off the skirt sticks out stiffly in the attacks; with it on the panels hang along the legs.

### All of Kratos's own clips (2026-10-03)

- Every `R_HERO*` WAD (00, 01, 02, 10–16; costume variants) holds the same `ANM_hero` (375–378 entries,
  189 groups such as `Navigation`, `Attacks`, `AirAttacks`).
- `tools/gltf_export.py … --clips all` exports every named transform clip: **191 animations** for Kratos,
  among them 18 `nav*` (idle, combat idle, walk slow/fast/strafe, jump, jump up, double jump, wall jump,
  fall, fall loop, land, high-fall land), 29 `att*` and 54 `mag*` clips. `analysis/levels/KRATOS_gltf`
  (8.3 MB, blades attached) plays them; `navWalkFast` shows a proper stride.
- Of the 891 animation names that the hero moves reference (`MOV+0x08`), 143 are clip names in
  `ANM_hero`. The others are not Kratos-only data: grab/kill animations live in each enemy's WAD
  (`navFall`/`navLand` appear in 76 WADs, enemy rigs) and traversal animations in the level WADs that use
  them (`wallClimbIdle` in 39, `ropeClimbU` in 26). Collecting those per level is open.

### Character patch records `ANM_hero_*` (2026-10-03)

- Level and enemy WADs carry **group-only** ANM records: `u16 3` at `+0`, group count 0 at `+0x12`, and
  one group at `+0x0c` (`+0x18` clip count, `+0x20` group name, `+0x38` hash, `+0x40` clip offsets relative
  to `+0x0c`). They add clip groups to Kratos's `ANM_hero` when the WAD is loaded and use its transform
  track layout. HIGH (layout; `ANM_hero_WallClimb` decodes with 2 bad of 326 length-test samples).
  `anm_decode.clips` and `rig_anim.clip_channels` handle them.
- 136 such records exist disc-wide (`analysis/hero_anm_patches.json`, record → first WAD): traversal sets
  in level WADs (`WallClimb` 22 clips, `WallGapLeap` 12, `Ceiling` 17, `Rope` 13, `Ladder` 4, `Grapple` 11,
  `Swimming` 12, `BalanceBeam`, `WallPress*`), sets per enemy in the enemy WADs (grabs, kills, minigames:
  `ANM_hero_Cyclop30`, `ANM_hero_Colsus01` …) and per boss (`ANM_hero_Zeusbig` 16, `ANM_hero_Perseus` 11),
  plus `ANM_hero_Pegasus` (33) and `ANM_hero_Icarus` (29). With them, **751 of the 891** animations that
  Kratos's moves name are found (143 before).
- `tools/gltf_export.py --extra-anm WAD:ANM_hero_X,…` adds them: `analysis/levels/KRATOS_gltf` now has
  **359 clips** (own set + traversal, swimming, Pegasus, Icarus), 17 MB.

### Checks made while porting to Rust (2026-10-03)

- **Rotation convention matches the game code (CONFIRMED).** `FUN_0010ab90` (disassembly, VU0 macro ops decoded by
  hand) builds each matrix row from the normalised quaternion as row 0 = `[1-2(yy+zz), 2(xy+zw), 2(xz-yw)]`, so
  row i of the game matrix is column i of the standard column-vector rotation matrix. That is the convention
  `tools/rig_anim.py mat_to_trs` and `gow2-skel` use. The four inputs are lanes x, y, z, w in slot order.
- **Rig and bind pose are right (HIGH).** For all 41 joints with at least 20 vertices, the chained bind-world
  position lies within 4.3 units of the centroid of the vertices bound to it (stored-as-world would be 20-34 off).
  Parent links, matrices and the per-vertex joint index are consistent. Bind-pose skinning cannot show a wrong
  rig, so this check is what confirms it.
- **Quaternion unit (MEDIUM).** Full four-component samples have length about 20,860 = 8 * 16384 / (2 pi), not
  16384 (median over 34,633 samples; a few outliers). Components a clip does not animate are filled from the bind
  quaternion; filled at the 16384 scale they are 21 % too small next to animated ones. Both fills (bind, or
  zero with a derived w) give almost the same standing pose, so this does not explain the odd poses.
- **Clip health.** 28 of the 219 named entries in `ANM_hero` are not clips (names like `TTTT...`, `42`, `er3A`); more than
  99 % of their segments fail to decode. They are filtered in the Rust viewer.
- **Pose sanity (HIGH for the numbers).** Standing clips put the lowest vertex at -2..0 (feet on the ground) and the
  head near 31-36; air clips lift the feet about 6; left and right leg data are near mirror images; knees are
  straight in idle. An independent glTF-standard skinning of the exported file matches the Rust skinning to 0.001.
- **Open (MEDIUM, seen on screen).** Some clips still look wrong when rendered (twisted torso in `navCombatIdle`,
  one leg bent back in `navIdle`, white cloth flaps near the shoulders and thighs); the Python viewer shows the
  same. Candidates: the 42 skirt/cloth joints (code-driven by `SCR_cloth`, not clip-driven), vertices that need
  more than one joint, which rotation components the game really leaves at rest values. Needs a reference
  frame from the real game (PCSX2) for one clip to compare against.
- **Hypotheses tested for the loose white strips (shoulders, thighs, 2026-10-03), all negative.** (1) Stretched
  triangles: in `attComboSlash01` frame 37 only 3 of 5,243 triangles grow by more than 6 units, and those are the
  chain between the two hands. (2) Skirt/cloth joints (rig 65-107): hiding their vertices leaves the strips.
  (3) The B material variants (`MAT_kratos1B/2B/3B`, 982 triangles): hiding them removes bracers and greaves, not the
  strips, so B is part of the armour set, not an alternate. (4) Texture decode: fixed (see formats.md); the strips
  remain. (5) Zeus renders cleanly with the same decoders. The bind pose is clean from the front. Still open: which
  joints own the strips (use `J`, joint colours, in `kratos-view`), whether some vertices need more than one joint,
  and whether the game draws these triangles at all (needs a PCSX2 reference frame).
- **Viewer tools** (`gow2-rs/crates/gow2-bevy/src/bin/kratos_view.rs`): `-`/`=` speed, `.`/`,` frame step, `R` in place,
  `F` follow camera, `J` joint colours, `K` hide skirt joints; `GOW_MODEL=zeus`, `GOW_HIDE=B` environment switches;
  optional arguments `<WAD> <clip> <time>`.

### Joint-local model parts and the armoured Kratos (2026-10-03). HIGH

- **Reference.** A PCSX2 screenshot of the first level (Rhodes palace) shows Kratos in the **God armour** (chest plate,
  spiked pauldrons, bracers) with the HUD (health bar green, magic bar blue, Blades-shaped frame, red orb count).
  That model is `R_HERO01.WAD` (`MDL_hero_0`, 7,683 vertices); `R_HERO00.WAD` (6,951) is the base look without chest armour.
- **Not every part is in bind-pose model space.** Part 0.0.0 of `R_HERO01` (869 vertices, the pauldrons, palette
  `lHumerus`, `lClavicle`, `rHumerus`, `rClavicle`) has a bounding box 6 x 7 x 6 centred 1.5 units from the origin and
  30 units from its joints: its vertices are **joint-local** and are placed by the joint's world matrix
  (`v * world(joint)`), not by `inv_bind * world`. Treating them as bind-space left the plates at the origin in the
  bind pose (an ornament between the feet) and floating beside the body in every clip.
- **Detection rule used (MEDIUM).** A part is joint-local when its bounding box centre is within 4 units of the origin and
  more than 12 units from the bind position of every joint its vertices use (`gow2_skel::local_parts`). This marks
  exactly part 0 of `R_HERO01` and nothing in `R_HERO00` or the other characters tested. The game's own flag for it is
  not found; candidates are the part header fields (`C+0x04`, `C+0x08`, `C+0x14`) and the hierarchy index `i` (0 here, 1 for the body).
- **Result.** The armoured model renders correctly in bind pose and in the lunge and combat-idle clips. The Python
  exporter (`gltf_export.py`) still treats every vertex as bind-space, so its Kratos and its R_HERO01 export keep this fault.
- **Still open on the base model (`R_HERO00`).** The loose white strips at the shoulders and thighs remain; they are not
  joint-local parts. Two GIF tags (`w` = `0x5028`, `0xb028`; palette slot 10 of parts 14 and 15) put vertices on a joint
  (`rWrist`, `lFemur`) 19 and 15 units from where they sit; whether these are mis-assigned or the strips are something
  else is not decided. Distance to a joint is a poor test (a vertex near the far end of a bone is nearer the next joint).

### Two-joint blended skinning (2026-10-03). HIGH for the structure, MEDIUM for the exact weighting formula

This replaces the rule "each vertex has weight 1 on the joint of its GIF tag". That rule produced the loose white strips on the
base Kratos, the floating greave plates on the armoured Kratos, and torn shoulders, knees and ankles in every animated pose.
The bind pose looks right under both rules, because every matrix cancels there.

- **Tag word `w` carries two palette slots.** The low bits `(w & 0x3ff) / 4` give the **base joint**; the high nibble
  `(w >> 12) & 0xf` gives a **second joint**. Across 975 tags of R_HERO00 the high-nibble slot fits the vertices better in 505 and
  the low-bits slot in 60 (R_HERO01: 420 vs 59, R_ZEUS: 855 vs 192); the rest have equal slots. Every "strongly mis-assigned" tag found
  earlier (assigned joint more than 10 units away, another palette joint under 4 units) is a lower-leg tag whose second slot is the
  child joint (tibia then metatarsal, femur then tibia): the weights were on the foot side.
- **Position `w` (low 15 bits) is a 12-bit blend weight** (4096 = 1.0): the pull of the second joint. Vertices with a weight above 0
  sit nearer the second joint 2,454 to 273 (R_HERO00) and 2,349 to 395 (R_ZEUS); weight 0 splits about evenly (they are on the base
  joint). Bit 15 stays the strip-break flag (`mdl_decode`).
- **Model used by `gow2-skel::skin_positions_blend`:** `p' = (1 - t) * (p * M[base]) + t * (p * M[second])`, `t = (pos.w & 0x7fff) / 4096`,
  with the skin matrix `inv_bind * world` (or the joint world matrix for joint-local parts). Result: the base Kratos (R_HERO00) loses
  the strips and both models keep their armour and greaves attached in every clip tested.
- **Open (MEDIUM).** The blend is linear in position; whether the game also blends normals, and whether `w` = 4096 means the
  second joint takes the vertex fully, is only supported by the fit statistics. Tags whose high nibble is 0 may mean "no second joint".
  The Python exporter (`gltf_export.py`) and its glTF files still use rigid weights; the glTF skin could carry the two joints as
  JOINTS_0 / WEIGHTS_0.

### ~~Partial rotation channels are deltas on the bind rotation (2026-10-03). HIGH (statistics and screen), order MEDIUM~~ (wrong; superseded by "Ground truth from RAM" below. It hung the arms only because the clavicle angles are large and the true rule is an absolute rotation vector.)

A rotation channel that stores fewer than four quaternion components (3 components with w implied, or 1 component) does not hold the
absolute local rotation: it is a **delta applied on top of the joint's bind rotation**. Channels with all four components are absolute.
This replaces the exporter rule "missing components come from the bind quaternion and the clip rotation replaces the bind rotation".

- **Evidence.** Per joint over 29 `att*`/`nav*`/`wpn*`/`mag*` clips (frame 0): the 3-component joints with a big bind rotation sit near
  identity in the clip, not near their bind (`lClavicle` bind 89.7 deg, clip 20.4 deg from identity and 70.5 from bind; `rClavicle` the same; thumbs F1/F2,
  both metatarsals, jaw). The 4-component joints are the opposite (femurs 125-135 deg from identity, 54-72 from bind; humeri, chains).
  Applying the delta rule to every joint instead gives garbage (all other modes tested look wrong), so it is specific to partial channels.
- **Measured effect (R_HERO01, `navCombatIdle` frame 0).** Upper arm angle from straight down: old rule 97.7 deg (arm held out horizontally),
  delta rule 15-24 deg (arms hanging, as in the PCSX2 reference of the first level). `navIdle` (R_HERO00): 90 deg to 4-12 deg; the feet are level
  (ankle heights 2.0 and 1.8) and flat (toe level with the ankle) instead of one toe pitched down 2.6.
- **Order.** `DeltaThenBind` = `R_row(delta) * R_row(bind)` (a delta in the joint's own frame, then the bind orientation): left/right upper arms
  4.4 and 7.0 deg in `navIdle`, against 12.4 and 5.9 for `BindThenDelta`. Both hang the arms; the first is the usual local-frame convention
  and is the default (`gow2_skel::Delta`, viewer variable `GOW_DELTA`).
- **Missing w.** With delta semantics the missing components are zero and `w = sqrt(U^2 - x^2 - y^2 - z^2)` (U = 20,860.8); this is `Fill::Implicit`.
- **Not yet confirmed.** The game's code path that applies the delta (the sampler writes the accumulators; the joint builder `FUN_0010ab90` takes them);
  whether 1-component joints (tibia, jaw, thumb F3) are deltas too (they are treated as deltas here); translation channels (the pelvis has one) are used as absolute.
  The Python exporter (`gltf_export.py`) still uses the old rule.

### Whole-body yaw in the clips (2026-10-03). HIGH for the measurement, MEDIUM for the interpretation

The pelvis rotation in many clips is a yaw of tens of degrees, and the shoulders follow it: `navIdle` hip line +43.9 deg, shoulder line +41.5;
`navCombatIdle` +54.4 and +50.1; `attBrutalSlash` -26.4 and -22.7 (the bind pose has 0 for both). So it is the whole body turned (a side-on stance),
not a hip twist. With that turn removed, `navIdle` has the feet side by side and level (ankles x = -7.1 and +7.1, z = +0.2 and -1.7, y = 2.0 and 1.7),
and `navCombatIdle` is a symmetric wide stance like the PCSX2 reference. With a fixed front camera the turn looks like odd, staggered legs, which is
what the first playtests reported. Leg data checked: thighs/shins are symmetric left to right in the body's own frame; all alternative femur rules
(delta, conjugate, either order) flip the leg upside down (ankles 22-39 units above the ground), so the femur is absolute.
- The game presumably turns the model to the character's heading, and the clip yaw is the stance's offset from that heading; how the engine
  combines them is not decoded. The viewer's `Y` key (default on, `GOW_RAWTURN=1` for off) undoes the hip-line yaw so a pose can be judged from the front.
- Pose checks run so far: arm angles, foot level and flatness, leg symmetry, hanging arms in the idles, plausible lunge and jump frames. Not checked:
  hands and fingers, the neck and head, the spine in attacks, translation of the pelvis over time, and every clip against the real game.

### Comparison with PCSX2 recordings (2026-10-03). `reference/combat_idle.mp4`, `jump.mp4`, `square1.mp4`, `triangle1.mp4` (30 fps, first level, God armour)

Tool: `tools/video_frames.py <video> --every N --out <dir>` writes frames and a numbered contact sheet (OpenCV, installed with pip).

- **Which clip is which.** The recording named `combat_idle` was made before the blades were drawn: it is the plain idle (`navIdle`; arms by the sides, torso upright,
  feet a little wider than the shoulders, fists near the thighs), not `navCombatIdle`. After the Square tap the game drops into the crouched combat stance
  (frames f60 to f108 of `square1.mp4`): wide stance, knees bent, torso leaning forward, arms low, blades hanging. That is `navCombatIdle`.
- **Standing idle: matches.** `navIdle` on `R_HERO01`, viewed from the front, shows the same stance, arm hang and shoulder turn as the game.
- **Combat stance: close, not identical.** Ours has the wide stance, bent arms and a lowered pelvis (y = 16.1 against 21.7 in bind), but the torso is more
  upright (pitch about 9-15 deg under every spine rule tried) and the legs look straighter and wider than the game's, which leans forward over bent knees.
  Camera is a factor: the game views from above and from his left with the body turned about 50 deg to the camera, so single stills mislead.
- **Attack timing.** The Square tap starts at about video frame 9; the clip (45 frames at 30 fps) ends at about f54 and the game then holds the combat stance.
  Matching moments: ours t = 0.1, 0.3, 0.5, 0.7, 0.9, 1.1 s correspond to video frames f12, f18, f24, f30, f36, f42. Game frames show a right-hand swing with the blade
  forward (f12), a low lunge with the blade pointing down (f18), a raised arm with the blade spinning (f24), then both arms low (f30-f42).
  An automated side-by-side of those moments was not completed (screen captures were unreliable while the machine was in use).

### Ground truth from RAM: rotation channels, exactly (2026-10-03). CONFIRMED

Method: the game's own joint matrices for Kratos were read from the PCSX2 RAM captures (`analysis/runtime/ingame1`, `ingame2`, the savestate), and the
decoder's pose was compared with them joint by joint. This replaces screenshot comparison and guessing.

- **Where the runtime skeleton lives (all three captures).** The 123 **local** joint matrices (row-vector, 0x40 bytes each, joint order = rig order) are at
  `0x952980`; the matching **world** matrices (Kratos at world (-1704.28, 3712.00, -5353.45)) are at `0x955260`. World = local x parent world holds for 113 of 123 joints
  (the rest involve the root node transform). `0x94cc10` holds another buffer of the same shape that is not consistent with the world array. Heap addresses are
  those of these captures; `tools/export_runtime_pose.py` writes them to `gow2-rs/crates/gow2-skel/tests/oracle/runtime_pose.tsv`.
- **Both captures play `navIdle`** (plain idle, blades away): the pelvis translation matches the clip to 0.01 units (runtime (1.00, 20.89, 0.15), clip (1.01, 20.89, 0.15)) at
  clip frame 9 (`ingame1`) and 18 (`ingame2`). So clip lookup, frame timing and translation decoding are exact.
- **Four-component rotation channels are quaternions, decoded exactly**: pelvis, neck, both humeri, radii, wrists, femurs have 0.0 to 0.1 deg error against the game.
  The convention (row i of the matrix = column i of the standard rotation matrix, raw x, y, z, w order, normalised) is exact.
- **Channels with fewer than four components are ROTATION VECTORS, absolute, not quaternion parts and not deltas.** Stored values x 2 pi / 16384 are the components of
  axis x angle in radians; q = (sin(theta/2) v/theta, cos(theta/2)). Checks: head stored 0.093 x 8 = 0.744 rad = 42.6 deg against a true 42.3 deg; vertebrae3 16.5 against 16.8;
  jaw 135.0 against 135.0; tibia 19.3 against 19.2; clavicle 80 against 83 about nearly the same axis. This is the same constant (0.0003834952) the matrix routine `FUN_0010ab90`
  uses, and the neighbouring routine `FUN_0010ad40` (small-angle threshold 0.008) is presumably the axis-angle to matrix builder. Missing components are 0, there is no bind fill.
  Result: vertebrae2 0.1 deg, jaw 0.0, both tibiae 0.0 to 0.1, vertebrae3 and 4 1 to 2 deg, head 1 to 3 deg. Mean body-joint error 8.2 deg (delta rule) to 2.9 deg.
- **What is still different from the game (constant across both captures).** Clavicles 19 and 28 deg, metatarsals 7 and 9 deg. These look like code-driven adjustments on top of the clip
  (a shoulder follow and foot placement on the ground are the likely ones), not decoding faults. Fingers, weapon and chain joints and the numbered skirt joints (cloth) are code-driven
  too and are not animated by the clips the way the body is.
- **Superseded readings** (kept, struck through above): "missing components come from the bind quaternion x 16384" (the Python exporter), "x, y, z with w implied", "delta on the bind rotation".
  The Python exporter (`gltf_export.py`) still decodes partial channels the old way.
- **Test.** `gow2-skel/tests/runtime_accuracy.rs` asserts four-component joints within 0.5 deg, the mean body error under 4 deg and the pelvis translation within 0.05, from the RAM-derived data.