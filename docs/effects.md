# Kratos's visual effects: move effects, footsteps, blade fire, trails

Status: first pass 2026-10-03 (pass7 decompilation, hero move data, RAM capture `ingame2`). Confidence
levels follow `docs/confirmed.md`. Particle internals (PTC_/FXC_ records, the op VM) are in
`docs/particles.md`; this file covers how Kratos's moves and weapons use them.

## 1. Effects are `go` particle rigs. HIGH (disc)

Every effect a move spawns is a **`go` record** (subtype 3, the same node type as level props,
`docs/animation.md` go placement) whose group holds an unnamed rig record, an `ANM_` clip (emitter rate
tracks, type 10) and its `FXC_` emitters, `PTC_` particle shapes and `MAT_` materials. Example
`gorundust` (`R_PERMA`): `FXC_dustemit` → `PTC_dustpart` (`MAT_normalsmoke`), a gravity field
`FXC_dustgrav`, driven by `ANM_rundust`.

Where they live (resolved by hash over all 293 WADs, `tools/move_fx.py`):

| Effect | WAD | Use on Kratos |
|---|---|---|
| `gosklblood` | `R_PERMA` | blood on grab/kill finishers (156 actions; joints `head`, `vertebrae4`, `pelvis`) |
| `gorundust` | `R_PERMA` | dust at the hands or feet (wrists 71, metatarsals 10): landings, slams, climbing |
| `gogoldenflash1..9` | `R_PERMA` | the golden flash over the body during the `MOV_Golden*` moves (one emitter per limb bone: `vertebrae4`, humeri, radii, femurs, tibias) |
| `goabsorbfx`, `goabsorbfx1/2` | `R_PERMA` | orb absorb glow (Golden Fleece absorb/reflect moves) |
| `gorockland`, `godirtland` | `R_PERMA` | landing debris |
| `gofxfirecore` | `R_WEAPON0_*` | blade fire burst (`MOV_SpecialEvade`, 4 bursts at 0.41–0.71) |
| `gocombo3fexplode`, `gofxlcombo3f`, `gofxrcombo3f`, `gofxfirepath` | `R_WEAPON0_*` | combo-3 fire finisher: explosion model + flash/cloud/sparks/debris, left/right blade fire trails animated along the swing (`ANM_fxl/rcombo3f`, 1,312-byte rigs) |
| `gohammersoulhit`, `gohammerwave`, `gobonepoisonexplode`, `goboneshotgun`, `goolympusbolthit` | `R_S_HAMMER*`, `R_S_BONE*`, `R_S_OLYMPUS*` | sub-weapon effects |
| `go<enemy>...` (≈150 more) | enemy / boss WADs | effects of grab and kill sequences against that enemy (Barbarian dust/blood, Colossus sparks, Cerberus, Zeus …) |

The lookup order at run time (`SCR_PlayFX`, `docs/scripting.md` §8): the character's own WAD, the two
loaded level WADs, the common WAD, so enemy effects are found while that enemy's WAD is loaded. HIGH.

## 2. Move-triggered effects: `tActionPlayFX`. HIGH (code + data)

`tActionPlayFX` (571 hero actions) is an action of kind 0x0d with class hash `SCR_PlayFX` at `+0x08` and
a **relative pointer at `+0x0c`** to its parameter block (the block `SCR_PlayFX` reads as instance
`+0x74`): `+0x00` effect name hash, `+0x04` attach joint hash, `+0x24..+0x2c` rotation in degrees,
`+0x30` flags. CONFIRMED: the pointer always lands on the 20-byte object that follows, and both hashes
resolve to real record and joint names.

`tools/move_fx.py extracted/pak/R_HERO00.WAD` writes `analysis/dc/R_HERO00/fx.tsv` (1,128 rows,
182 distinct effects, 5 hashes unresolved). Flag patterns: `0x10` (follow the joint) for dust and
absorb glows, `0x18` (follow + sync to the animation) for the golden flashes, `0x90` for finisher blood,
`0x02` (lifetime tied to the target) for the combo explosions.

## 3. Footstep effects: `tActionFootEffect` (kind 0x1d). HIGH (code)

557 actions (`MOV_Walk`, runs, attacks with steps). Action `+0x08` flags, `+0x0c` foot joint hash
(`lMetatarsal` / `rMetatarsal`). Case `0x00248cd4` → `FUN_002600e0` → **`FUN_0025fd80` Character_FootEffect**:

1. **Surface**: with the character flag `+0x37c & 0x400000` the game casts a short vertical segment
   (±6 units) through the foot joint into the collision world (`FUN_0011f608`); the hit triangle's
   material gives a surface record (`FUN_001204f0`). Otherwise the stored ground material
   (`+0x270` / `+0x264`) is used.
2. **Dust**: the surface record `+0x10` names an effect, spawned at the foot (`FUN_002667d0`; surface flag
   `+0x2b & 8` attaches it to the joint, `& 4` uses the character's rotation).
3. **Sound**: the surface record `+0x1c` names the footstep sound (`SND_FOOTSTEP_<surface>`). The gait goes
   to the character's sound emitter as **register 0** (`FUN_0017e7f0(emitter, 0, gait)`, then
   `FUN_0017d758` plays it): the action's low nibble 1–4 forces step / walk / run / land; nibble 0 picks
   it from speed: below ⅓ of the run speed (`+0x164` × 16) step, below ⅔ walk, else run. The sound bank
   program branches on that register (`docs/audio.md` §4.2). CONFIRMED (code and bank agree).
4. Flags `0x100`/`0x200` add a ground mark at the joint (`FUN_00271148`, splash/ripple, MEDIUM).

`tActionCliffEffect` (kind 0x1f, 109 actions) is the same for wall and ceiling contact (`+0x170 & 0x20`
wall, `& 0x800` ceiling): byte `+0x08` 0/1/2/3 sets register 0 to 4/5/6/0 and plays the wall or ceiling
surface's sound (`FUN_00260128`). HIGH.

## 4. Blade fire and lights. HIGH (disc), MEDIUM (runtime use)

The blade object `gomaiblade` (`R_WEAPON0_<level>`) carries its own effects in its rig group:
`FXC_BDepoly6` / `FXC_BDepoly3` (subtype 13: ~~curves along the blade~~ meshes of the blade, `MSH_BDepoly*Shape`)
feeding `PTC_flame6` / `PTC_flame3` (the flames licking along the blades) through `FXC_BDEsparkemit` /
`FXC_BDEsparkemit.0` (subtype 3, ~~sparks~~ surface emitters with normal speed 0.5; `docs/particles.md` §4.2). Two point lights
`LeftBladeLight` / `RightBladeLight` (LightServer 6) are looked up by the weapon code
(`FUN_0023df58`, `Obj_FindByName("LeftBladeLight")`) and follow the blades: the blades light Kratos and
nearby characters (`docs/rendering.md` §1: only characters receive lights).

## 5. Trails. HIGH (code)

Two kinds of ribbon trail, both FX subtype **0xb** objects (descriptor `0x000b0019`: server 0x19, subtype
11) built by `FUN_00139900` with the same parameters: material name, 160 samples (`0xa0`), 0.32 s
(`0x3ea3d70a`), identity matrix.

- **Blade trails** (the swing arcs): the chained weapon constructor `FUN_0023cf28` (strings `MAT_swordtrail`,
  `MAT_chainlink`, `MAT_chainglow`, `Pelvis`) builds one per weapon (`+0x128`, instance `+0x12c`) with
  `MAT_swordtrail` from `WAD_R_Weapon` (`FUN_0023de88`), or **`MAT_godswordtrail` / `MAT_godchainlink` /
  `MAT_godchainglow` from `WAD_R_Perm` in god mode** (`FUN_0023ddb8`). Each frame the weapon update
  (`FUN_0023e950`) sets the trail colour (`FUN_0013a250`) from a global table indexed by the weapon level
  (`0x335868` byte 1, `g_GblGlobal+0xb8`, 0x20 bytes per level) or by the god-mode level (byte 2,
  `+0xbc`, 0x50 bytes per level). The trail draws while the attachment flag `+0x18 & 0x10` is set
  (`FUN_00246680`: `FUN_0013a240(trail, flags & 0x10)`); the swing code clears it when the blade comes
  back (~~MEDIUM: the setter is not read~~ the chained update sets it each frame and clears it when the
  blade is stowed on the back, `docs/animation.md` "Blade attachment", HIGH).
- **Body trail** (`Hero_Create`, `+0x1a0`/`+0x1a4` of the player controller, `MAT_swordtrail`): updated by
  `FUN_00215860` each frame between the **head** (character `+0x28c` = joint 7) and the **midpoint of the
  two feet** (`+0x3e4`/`+0x3e6` = 108/112 `lFemur`/`rFemur`, + 2 = `lMetatarsal`/`rMetatarsal`; RAM
  `ingame2`, CONFIRMED), with an alpha from its projected screen size (/512, clamped to 1). It is switched
  on only when the move flag `0x2` is set (`tActionSetFlags`, entry `+0x2b0`): in the hero data **only the
  grapple swing moves** (`MOV_GrappleEnter`, `MOV_GrappleSW1..4`, `MOV_GrappleJumpF/B`,
  `MOV_GrappleCircle`). So it is the streak behind Kratos's body while he swings on the grapple. HIGH.

### 5.1 The trail renderer (FX subtype 0xb). HIGH (code), MEDIUM (template shape)

Object built by `FUN_00139900` → `FUN_001395f8`; per-frame driver `FUN_00281ec0`. Fields: `+0x13c`
vertices per sample, `+0x144` ring size (160), `+0x150` trail clock, `+0x154` stream flags (1 = colour,
2 = UV, 4 = use a fixed time step `+0x170`), `+0x158` **on** flag (`FUN_0013a240`), `+0x160` colour,
`+0x174/+0x17c` ring tail/head (one pair per double-buffered vertex set, `+0x147` of the mesh picks the
buffer), `+0x188` per-sample time stamps (−1 = empty), `+0x18c` the dynamic mesh.

Each frame:
1. The clock advances by the owner's frame time.
2. **While on**, the attachment's previous and current matrices are interpolated into up to **10 sub-samples**
   (count from the motion, `FUN_0013aec0`, capped at 10). For each, `FUN_0013a508` transforms the trail's
   template points (record `+0x20`) by the interpolated matrix, stores them as integer positions in the next
   ring slot and stamps the slot with the time. A full ring overwrites the oldest slot.
3. **Age-out**: tail samples older than the lifetime (record `+0x14`, 0.32 s for the blade and body trails)
   are cleared (`FUN_0013a298`: position 0x8000, time −1).
4. **Streams**: colour (flag 1) = colour × 128 with alpha rising linearly from 0 at the oldest live sample
   to full at the newest; UV (flag 2) = u `(1 − age/lifetime) × 4096` along the trail, v
   `i / (n − 1) × 4096` across it (12-bit fixed point).
5. When the head meets the tail in both buffers the clock resets to 0.

So when the flag goes off the trail is not cut: it stops growing and fades out over 0.32 s. The blade trail's
colour comes from the `GBL+0xb8` table (RAM `ingame2`: (1, 1, 1) with 0.8 and a per-level factor 1.0, 1.5,
2.0, 2.5, 3.0, 1.0 for weapon levels 0–5; MEDIUM which field is width or brightness). God mode uses the
0x50-byte entries at `GBL+0xbc`.

**Viewer.** `analysis/levels/viewer.html` ("blade trails") draws this for the Kratos export: 160 samples
of two blade edge points, up to 10 sub-samples per frame, 0.32 s lifetime with alpha fade, white additive,
on whenever the blade is in the hand or out on the chain. The two edge points are a viewer choice (the ends
of the blade mesh's long axis); the game's template points are not decoded.

## 6. Effect gallery (viewer)

`analysis/levels/fx.html` plays one `go` effect at a time at the origin, from
`analysis/levels/R_PERMA_fx/particles.json` and `analysis/levels/R_WEAPON0_0_fx/particles.json`
(`tools/ptc_export.py`): 68 shared effects (blood, dust, surface hits `go<rock|wood|metal|dirt|sand>hits/slice/land`,
golden flashes, orbs, god-mode glows) and 5 Blades-of-Athena effects. Rate tracks loop with a 0.6 s gap.
Viewer approximations: emitters without a rate track run continuously at their stored rate (in game the
effect's clip gates them); particle size is drawn as a world half-extent (the screen-size scale is open,
`docs/particles.md` §6).

## Open questions

- ~~The trail renderer itself (FX subtype 0xb, `FUN_00139a20`, `MAT_trail1` default) and how the 160 samples
  are aged into the ribbon; the colour table values in `g_GblGlobal`.~~ Done (§5.1). Still open: the trail
  template points (record `+0x20`) and the meaning of each colour-table field.
- ~~Which code sets the attachment flag `0x10` (blade trail on).~~ The chained update `FUN_0023e950` sets it
  every frame and clears it in mode 0 (blade on the back), so the trail runs whenever the blade is in the
  hand or out on the chain (`docs/animation.md` "Blade attachment"). HIGH.
- ~~Emitter subtypes 1, 3, 7, 8 and 13 (`FXC_dustemit`, `FXC_BDEsparkemit`, `FXC_BDepoly*`) are not yet in
  `tools/ptc_export.py` (it reads subtypes 2 and 5), so the viewer cannot play these effects yet.~~ Done
  (§6). Still open: ~~the curve emitter (subtype 3 along the subtype-13 curve)~~ done: subtype 3 is the
  surface (mesh) emitter and subtype 4 the curve emitter (`docs/particles.md` §4.2); ~~and the volume spawners 5–10
  are drawn with the cone~~ (volume spawners ~~5–8 and 10~~ 5–9 done, `docs/particles.md` §4.1); the blade emitters sit in the blade's 1/64-scaled model space.
- The surface record (`FUN_001204f0`) layout beyond `+0x10`, `+0x18`, `+0x1c`, `+0x20`, `+0x2b`.
- Hit sparks/blood on ordinary hits: not in the hero's moves; probably the victim's reaction data.
