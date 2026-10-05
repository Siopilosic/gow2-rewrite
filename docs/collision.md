# Collision world and character movement collision

Status: first pass (2026-10-03). Evidence: Ghidra pass5 decompilation. No runtime capture yet.
Confidence levels follow `docs/confirmed.md` (CONFIRMED / HIGH / MEDIUM / LOW).

## 1. The collision world

`DAT_002d7870` points to the level collision world. Every query in this document passes it as the first argument (HIGH).

| World offset | Meaning | Level |
|---|---|---|
| `+0x83c` | candidate list: up to 0x1ff collider pointers, zero-terminated | HIGH |
| `+0x1058` | first free slot in the candidate list | MEDIUM |
| `+0x103c`, `+0x105c` | broadphase root / current node; when they match, the filtered gather `FUN_00181190` runs, else `FUN_001810f8` | MEDIUM |
| `+0x1060` | dirty flag; non-zero makes `FUN_0011ef00` rebuild before a query | MEDIUM |
| `+0x1064` | non-zero while a cached gather is active (the list is reused as is) | MEDIUM |

### 1.1 Query parameter block (`0x003636f0`)

All queries read a global parameter block. The caller fills it before every call.

| Address | Meaning | Level |
|---|---|---|
| `0x3636f0` | collide-with mask; usually `~(obj+0x284 \| 0x1eb1ff7)` | HIGH |
| `0x3636f4` | query flags; characters OR in `obj+0x288 & 0x4300000` | MEDIUM |
| `0x3636f8` | per-hit callback; returns `0xb` = ignore this hit, `0xc` = stop the query | HIGH |
| `0x3636fc` | mode; bit 1 = keep the nearest hit only | HIGH |
| `0x363704` | owner to ignore (the querying object's handle, `obj+0`) | HIGH |
| `0x36370c` | callback user data | HIGH |
| `0x363700`, `0x363708`, `0x363710` | always cleared by callers; meaning open | LOW |

### 1.2 Hit record (`0x002fd0a0`)

| Address | Meaning | Level |
|---|---|---|
| `0x2fd0a0` | contact point (vec4) | HIGH |
| `0x2fd0b0` | contact normal (vec4) | HIGH |
| `0x2fd0c0` | distance / segment parameter; reset to FLT_MAX | HIGH |
| `0x2fd0c4` | collider hit | HIGH |
| `0x2fd0d0` | surface id (u16) from shape vtable `+0x2c` | MEDIUM |

### 1.3 Collider objects

A collider in the candidate list has `+0x18` = owner (compared with the ignore owner) and `+0x44` = shape interface. The shape interface is called through a vtable with this-adjust pairs (`short` offset, function):

| Shape vtable | Use | Level |
|---|---|---|
| `+0x18` / `+0x1c` | sphere test, writes the closest point | HIGH |
| `+0x28` / `+0x2c` | surface id | MEDIUM |
| `+0x30` / `+0x34` | segment test; returns a hit code, `0xc` stops | HIGH |

### 1.4 Query functions

| Function | Reading | Level |
|---|---|---|
| `FUN_0011f508(world, params)` | gather candidates into the list at `+0x83c`; returns the list start | HIGH |
| `FUN_0011ff98(radius, world, centre)` | open a cached gather for a sphere (radius 32.0 or 40.0 in character code) | MEDIUM |
| `FUN_00120090(world)` | close the cached gather | MEDIUM |
| `FUN_0011f608(world, p0, p1)` | **segment query**. Skips segments with squared length < 0.0001. Calls every candidate's segment test. In nearest mode it keeps the smallest distance in the hit record. Returns the OR of hit codes | HIGH |
| `FUN_0011fd18(radius, world, centre)` | **sphere query**. For each candidate the sphere test gives the closest point; the hit record gets point, normal (centre minus point, normalised) and distance; then the callback runs. Returns the count of hits not ignored | HIGH |

## 2. Character movement collision (`FUN_00226af0`)

`FUN_00226af0(dt, character, target_pos, push_out, ground_probe, check_ledge)` moves a character to a target position with collision. Four callers. Level HIGH for the structure, MEDIUM for the helper names.

1. Mask: `0x8020000`; `0x48020000` unless the character is in wall state (`+0x170 & 0x20`) without `+0x374 & 0x80`.
2. Run the move-system actions once per frame (same pattern as Character_Update §2). If any action has flag `0x600000`, add `0x20000000` to the mask.
3. Up to 3 iterations:
   - optional push-out from other characters: `FUN_0021df20` (sphere query with callback `FUN_0021d9c0`; returns `0x400` when moved);
   - ground probe `FUN_0021d150`;
   - horizontal sweep `FUN_0021e998`; on contact the state word can switch to ground (`1`) or air (`8`);
   - vertical resolve `FUN_0021edd0`;
   - segment query from the old position (`+0x110`) to the new one. No hit ends the loop. A hit moves the target to contact point + 0.05 · normal and repeats.
4. If `+0x37c & 8`: water check `FUN_001f8738`; in water state (`0x380`), when the object at `+0x3c4` has a record at `+0xa4` with flag `0x10`, clear its `+0xac` and the character's `+0x340`.
5. If `check_ledge` and an action has flag `0x4000`: segment 32 units straight down. No hit: release `+0x1c8` (`FUN_00250430`) and start a fall (`FUN_00225780(dt, character, 2)`).

### 2.1 Helpers

| Function | Reading | Level |
|---|---|---|
| `FUN_0021df20` | character push-out (sphere around the capsule centre `+0x60 · +0x130`, radius from `+0x120`) | HIGH |
| `FUN_0021d9c0` | push-out callback: ignores non-characters, characters on Pegasus (bit 14), attached characters (bit 13) and self | HIGH |
| `FUN_0021d150` | ground probe; wall state goes to `FUN_0021d6f0`, ceiling state (`0x800`) to `FUN_0021d038`; other states probe several points with `FUN_0021cf70`; returns `0x20` on wall contact | MEDIUM |
| `FUN_0021e998` | horizontal sweep with slide; records contacts with `FUN_0021ce28(this, hit)` | MEDIUM |
| `FUN_0021edd0` | vertical / step resolve (gather radius 40) | MEDIUM |
| `FUN_00225780` | enter fall: state word low half = `8` (air), animation `0x67`, `+0x164 = 0x551184e7` | MEDIUM |
| `FUN_001f8738` | water surface check; resets `+0x260` to FLT_MAX, may enter water (`FUN_001f6db0`) | MEDIUM |

## 3. The level collision sheet (`RIB_sheet`)

Added 2026-10-03. Evidence: the loader `FUN_00179338` (pass5) and the polygon tests around `FUN_00179948`/`FUN_0017a7d8`/
`FUN_0017a968`, checked against the data of all 123 level WADs that have one (every level has exactly one record named
`RIB_sheet`). Reader: `gow2-formats/src/sheet.rs`; independent Python reader: `tools/sheet_decode.py`; the two agree on counts,
surface lists and a checksum of every polygon in every level (`tests/sheet_oracle.rs`).

The record is the static world collision: the shape the collision world `DAT_002d7870` queries for terrain. Its payload (CONFIRMED
by the loader and the data):

| Offset | Meaning |
|---|---|
| `+0x00` | type `0x10`, `+0x04` record size |
| `+0x10`, `+0x20` | bounds min and max (f32 x3); they equal the extent of the vertices in every level |
| `+0x3e`, `+0x40`, `+0x42` | triangle, quad and vertex counts (u16) |
| `+0x48`, `+0x4a` | surface count, flag-name count (u16); `+0x44` is the fallback surface stride |
| `+0x50` | eight section offsets (u32) from the record start; the eighth is the end |

| Section | Content |
|---|---|
| 0 | broad-phase nodes (not decoded; the polygons can be used without them) |
| 1 | surfaces: `+0x48` records of 64 bytes (the game divides the section size by the count) |
| 2 | flag-name table: `+0x4a` records of 76 bytes |
| 3 | leaf polygon lists: u16 entries, bit 0 = quad, the rest the polygon index |
| 4 | triangles, 8 bytes: u16 `surface << 4`, u16 v0, v1, v2 |
| 5 | quads, 10 bytes: u16 `surface << 4`, u16 v0, v1, v2, v3, a convex loop |
| 6 | vertices, 12 bytes each: f32 x, y, z in world units |

At load the game builds a 16-byte plane per polygon (normalised normal and offset). Triangles use `(v1-v0) x (v2-v0)`, quads
the cross product of the diagonals `(v2-v0) x (v3-v1)`. With those formulas the quad under Kratos's RAM-captured start position
(-1704.28, -5353.45) is a floor at exactly y = 3712.00 with normal (0, 1, 0) (CONFIRMED against the captured start height).

### 3.1 Surfaces and flag bits

A surface record: `+0x00` name (24 bytes), `+0x18` flag word (low), `+0x1c` flag word (high), `+0x28` doubleSided, `+0x2c`
MaterialFX id, `+0x30` PartType, `+0x34` WallHangTimer, `+0x38` Damage, `+0x3c` AIDamage. The flag-name table gives the schema:
49 entries, the first four being the word names `bitfield0` to `bitfield3` that the collision world looks up by name
(`FUN_0011fee8`) and the rest the named bits. Bit numbers (HIGH: they agree with the surface names and with the query masks):

| Bit | Name | Bit | Name | Bit | Name |
|---|---|---|---|---|---|
| 0 | Ground | 16 | AIBlock | 27 | CombatGuide |
| 1 | Water | 17 | ClimbGuide | 28 | NarrowNoBB |
| 2 | Narrow | 18 | PushPullSlide | 29 | NoCSMCollision |
| 3 | Climbable | 19 | BackPress | 30 | HODLeap |
| 4 | Ladder | 20 | NoPlayerCollision | 32 | TakeDamage |
| 5 | Slide | 21 | NoAICollision | 33 | NoIK |
| 6 | Death | 22 | NoPlayerUse | 34 | NoDecals |
| 7 | DeathSink | 23 | NoAIUse | 35 | DarkSurface |
| 8 | Ceiling | 24 | GeneralGuide | 36 | NoWallHang |
| 9 | NarrowWallPress | 25 | NoDiving | 37 | CamRelControl |
| | | 26 | NoPushPullCollision | 38-45 | NoLadderSliding, NoBackJumps, NoShadows, NoCeilingDrops, NoPullUps, NoDropDowns, ForceCollisionChecks, AutoPullUp |

Checks: surface `noPlayerSheet` has bits 0, 20, 22 (Ground, NoPlayerCollision, NoPlayerUse); `CombatGuide` has bits 21, 23, 27;
`GeneralGuide` bit 24; `Ladder` bit 4; `groundPlainWood` and `noWallHang` have bit 36 (NoWallHang). Bits 32 to 63 live in the second
word, which the collision tests never read.

### 3.2 How a query uses the flags

Every polygon test in the shape (`FUN_00179948` and the two scan loops) first reads the surface record and computes
`*(u32 *)(surface + DAT_002d7874) & DAT_003636f4`, where `DAT_002d7874` is the offset of `bitfield0` and `DAT_003636f4` the
query flags. The polygon is tested only when the result is zero: **the query flags are a skip mask**. The character code sets
them as `base | (character +0x288 & 0x4300000)` (HIGH; callers in `FUN_0020ce70.c` and `FUN_002212c0.c`), and `0x4300000` is
exactly bits 20, 21 and 26, the three `No...Collision` flags. Bases seen:

| Base | Skips (besides the character's own bits) | Used for |
|---|---|---|
| `0x10042` | Water, Death, AIBlock | floor and step probes |
| `0x41010042` | Water, Death, AIBlock, GeneralGuide, HODLeap | horizontal sweeps |
| `0x49030042` | the horizontal set plus ClimbGuide and CombatGuide | wider checks (purpose of each caller not traced) |

So for walking: guides (`GeneralGuide`, the camera sheets) do not block, `CombatGuide` walls do, and the hero's own `+0x288` very
probably carries `NoPlayerCollision` (the name and the mask agree; the value in the hero is not read from RAM, MEDIUM), so
`noPlayerSheet` polygons do not block the hero. The port implements exactly these masks (`sheet::flag::SKIP_FLOOR`, `SKIP_WALK`,
`HERO_OWN`).

### 3.3 RHOD10 contents (991 polygons: 330 triangles, 661 quads, 1,309 vertices)

| Surface | Polygons | What it is |
|---|---|---|
| `groundPlainRock` | 634 | the stone floors and walls (296 up, 32 down, 306 wall) |
| `groundPlainWood` | 134 | wooden parts |
| `sheetStrip_groundPlainC` | 126 | floor strips at y = 3712 to 3714, z -5597 to -5143 |
| `noWallHang` | 29 | rock and wood with the NoWallHang bit |
| `GeneralGuide`, `sheetStrip_GeneralGuide` | 24, 10 | invisible guide walls around the hall |
| `noPlayerSheet` | 13 | walls the hero ignores |
| `groundMetal` | 12 | metal |
| `Rhod10Cams_*` | 1, 4 | camera guide sheets |
| `Ladder` | 2 | a ladder (bit 4, not ground) |
| `CombatGuide` | 1 | combat arena wall |
| `col0_groundPlainDesign` | 1 | a floor at y = 2816 |

## Open questions

- ~~Shape types behind the collider vtables (mesh, box, capsule); format of the level collision data in the WAD.~~ Answered for the
  static world: the sheet, section 3. Still open: the dynamic colliders (`CDV_`/`CDZ_` ball hulls on characters and doors).
- ~~Meaning of query flags `0x10042`, `0x41010042`, `0x49030042` bit by bit.~~ Answered in section 3.2 (they are skip masks over
  the surface flag bits). The caller-supplied extra bits (`param_3`, `param_4`) are not traced.
- Broad-phase sections 0 and 3: a spatial tree over the polygons. Not needed to query the polygons; decode only if exact hit
  ordering is wanted.
- The surface fields `+0x28` to `+0x3c` (doubleSided, MaterialFX, PartType, WallHangTimer, Damage, AIDamage) are named by the
  schema but their effects are not traced. All 13 RHOD10 records hold the same PartType (8) and no damage.
- `0x363700`, `0x363708`, `0x363710`.
- Runtime check of the hit record during a jump (needs an in-level savestate).
- Whether polygons are one-sided in the segment test. The port treats walls as two-sided.
