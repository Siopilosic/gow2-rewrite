# Magic: casting path and the abilities

Status: first pass 2026-10-03 (pass7 decompilation, hero move data `R_HERO00`, RAM capture `ingame2`); Electric, Earth, Wind and Medusa natives read 2026-10-03 (§4–§8).
Confidence levels follow `docs/confirmed.md`. The data names (`Lightning`, `Electric`, `Wind`, `Earth`,
`Medusa`) are the names in the game data; retail ability names are not assigned (see
`docs/kratos-data.md` §6.7, which also has the item ids, level bytes and the cast branches).

## 1. From button to spell. HIGH

1. **Cast branch.** Button code 6 (pressed, or held for Wind and Medusa) from a combat move, with the
   selected magic `0x00335873` equal to the item id and `FUN_00210728` allowing it (meter check,
   `docs/kratos-data.md` §6.7), enters the ability's `*Enter` move (`MOV_LightningEnter`,
   `MOV_EarthEnter`, `MOV_WindEnter`, `MOV_ElectricEnter`, `MOV_MedusaEnter`, and the `*AirEnter`
   variants).
2. **The move runs script natives.** Action kind 0x0d (the 25 "object" action types of
   `docs/kratos-data.md` §6.5) carries at `+0x08` the **hash of a script class name** (`Hash_Name`, the
   same hash as the name dictionary) and at `+0x0c` a self-relative pointer to the class's parameter block.
   `MOV_LightningEnter` has, for example: `SCR_HideSubWeapon` at time 0, `SCR_ClearRadius` at 0,
   `SCR_Lightning` at 0.3, camera shake `CSH_GIANT` and rumble `FFB_GIANT` at 0.35, four sounds.
3. **`FUN_00249210(move, hash, params, action)` Move_RunScript.** Finds the class descriptor by hash
   (`FUN_001872a0`), builds a script object from it (`FUN_0013f300`) under the owner's WAD context, adds
   it to the ScriptServer (virtual `+0x2c`) and fills the instance:

   | Instance | Value |
   |---|---|
   | `+0x64` | handle of the owner character's node (the **target** the natives read, `docs/scripting.md` §5) |
   | `+0x68` | the move instance |
   | `+0x6c` | the action record (natives read their fixed parameters from it, e.g. `SCR_Teleport` at `+0x14`) |
   | `+0x70` | the class hash |
   | `+0x74` | the per-class parameter block (`SCR_PlayFX` reads it, `docs/scripting.md` §8) |

   It then activates the script (server virtual `+0x34`) and links it into the move instance's effect
   list (`move+0x48`), so it is tied to the move's lifetime.

So abilities are script natives started by move actions; the move data supplies timing, camera, rumble
and sound, and the native does the ability.

## 1.1 The magic meter. CONFIRMED (code + RAM)

The meter is `0x00335840` (meter selector 1 of `tActionMeterAdjust`, `docs/combat.md` §6).

- **Check** `FUN_00216530(cost, player)`: allowed when `meter ≥ cost × Difficulty[d][+8]`.
- **Spend** `FUN_00216418(cost, player)`: `meter −= cost × Difficulty[d][+8]`, clamped to
  `[0, capacity]`; adds the rounded cost to statistics counter 12 (`0x0036b0c8[12]`, magic used).
- **Capacity** = `s16` table at `GBL_Global+4`, indexed by the byte `0x00335867` (the magic upgrade
  level): 0, 100, 125, 150, 175, **200** (level 5 in the capture, meter full at 200).
- Both are bypassed when `DAT_002d86a8` is set and Kratos is not on Pegasus (an infinite-magic switch).

**Difficulty table** (`GBL_Global+0xc4`, stride 0x38, row = `DAT_002d86e0`; row 1 in the capture):

| Row | `+0` player damage (`docs/combat.md` §4.1) | `+4` | `+8` magic cost factor | `+0xc`, `+0x10`, `+0x14` |
|---|---|---|---|---|
| 0 | 2.0 | 0.5 | 0.75 | 1.5, 1.5, 1.5 |
| 1 | 1.0 | 1.0 | 1.0 | 1.0, 1.0, 1.0 |
| 2 | 1.0 | 2.5 | 1.0 | 0.75, 0.75, 0.75 |
| 3 | 0.75 | 5.0 | 1.0 | 0.25, 0.25, 0.25 |

`+4` grows with the row (0.5 to 5.0): MEDIUM that it scales the damage Kratos takes. `+0xc..+0x14`
shrink with the row: MEDIUM that they scale pickups or regeneration. Row names are not assigned.

With the Lightning cost of 33 at row 1 a full meter (200) pays for six casts.

## 2. `Scr_Lightning` (`FUN_001a5830`, 3,036 bytes). MEDIUM (first pass)

Registered by `FUN_001a6a50` (`analysis/scr_registry.tsv`). Strings: `goLghtnMain`, `goLghtnHide`,
`goLghtnRadius`, `goLghtnShoulder`, `goLghtnWrist`, `Lhumerus`, `PB_CircleBtnSmash`.

- **Stage data:** `GBL_Global+0x110` (self-relative), stride 0x60, indexed by `Item_GetLevel(1)`.
- **First activation** (instance flag `+0x2c & 0x4000` not yet set): installs the release callback
  `FUN_001a5798`, counts users in `DAT_002d82c0`, and for the first user creates the effect objects
  (`FUN_0013e480` on the descriptors at `0x2eafc0`…`0x2eb058`) and attaches six of them to Kratos's joints
  found by name (`Lhumerus` and the `goLghtn*` attach points, `FUN_001456b0` + `FUN_0013d1b0`). It clears
  the bolt state arrays at `0x00330328` (5 entries), and **pays the cost**: `FUN_00216418(stage[3],
  player controller)`.
- **Button mash (Circle):** when stage flag `stage[6] & 1` is set, a `PB_CircleBtnSmash` prompt is shown
  (`FUN_0025ac00(4, hash)`), each new press of pad bit `0x20` adds 1 to the charge `DAT_0036a880`, and the
  charge decays by `stage[0x14]` per second (scaled by the frame time `DAT_002d80dc · DAT_002d80d0`),
  clamped to `[0, stage[0x15]]`.
- **Power from the charge:** with `c = charge / stage[0x15]`:
  - damage = `(c · (stage[0xe] − 1) + 1) · stage[0]` (`stage[1]` instead when `DAT_002d86b4` is set);
  - count = `(c · (stage[0xf] − 1) + 1) · s8(stage[2])`;
  - radius/range = `(c · (stage[0x11] − 1) + 1) · stage[5]`.
  At full charge each is multiplied by the stage's factor.

**Values in RAM `ingame2`** (all three stages read the same in this capture):

| Index | Value | Use |
|---|---|---|
| `[0]` | 5 | damage per bolt |
| `[1]` | 25 | damage in the alternate mode (`DAT_002d86b4`) |
| `[2]` | 10 (s8) | bolt count |
| `[3]` | 33 | **meter cost** per cast (`FUN_00216418`) |
| `[5]` | 5 | range/radius (m) |
| `[6]` | 1 | flag: Circle mash enabled |
| `[0xe]`, `[0xf]`, `[0x11]` | 2, 2, 4 | full-charge multipliers for damage, count, range |
| `[0x14]` | 1 | charge decay per second |
| `[0x15]` | 10 | maximum charge (presses) |

## 3. ~~Other abilities (raw stage data, field meanings open)~~ Superseded by §4–§8 (2026-10-03)


~~The RAM capture holds the stage tables of the other abilities (`GBL_Global` offsets from~~
~~`docs/kratos-data.md` §6.7). Only a few fields are recognisable without reading their natives:~~

| Ability | `GBL` | Stride | Recognisable per-stage values (stages 1, 2, 3) |
|---|---|---|---|
| ~~Electric~~ | ~~`+0xf4`~~ | ~~0x5c~~ | ~~10 / 12.5 / 16.5 and 200 / 300 / 400 (damage and range or duration candidates)~~ |
| ~~Wind~~ | ~~`+0x114`~~ | ~~0xb8~~ | ~~10 / 15 / 15 and 0.33~~ |
| ~~Earth~~ | ~~`+0xf0`~~ | ~~0x68~~ | ~~first field 20 / 25 / 30; 100 or 250 and 500 or 750~~ |
| ~~Medusa~~ | ~~`+0xe8`~~ | ~~0x4c?~~ | ~~12, 0.5, 2.5, then 100 / 200 / 300~~ |

The table above guessed field meanings from the values alone; §4–§8 read them from the natives.

## 4. Shared pieces of the magic natives

- **Stage rows.** Each ability's natives read `GBL_Global+<offset>` (self-relative pointer) and index it by
  `Item_GetLevel(item id)`: rows 0, 1, 2 are the three upgrade levels (row 3 is already other data in RAM
  `ingame2`). HIGH (code), values from RAM `ingame2`.
- **Pad bits.** The natives test the input mask (`+0x28` current, `+0x2c` previous) directly. With the
  move-data button codes (`docs/kratos-data.md` §6.6): `0x80` Square, `0x10` Triangle, `0x20` Circle.
  `0x20` = Circle is HIGH: the Lightning and Earth natives show the `PB_CircleBtnSmash` prompt and count
  presses of bit `0x20`. Square and Triangle are MEDIUM (from the move names).
- **Follow-up moves by event.** A native picks the next move by posting an event code
  (`FUN_0024f988(move system, code, ...)` then `MoveSys_QueueBranch`); the ability's moves branch on codes
  `0x1c`–`0x20` (`analysis/dc/R_HERO00/branches.tsv`). The code → move tables below are CONFIRMED (data).
- **Blast** `FUN_00199918` (10 callers; MEDIUM name **Blast_Spawn**): spawns a blast record's effect at a
  matrix and a script object whose updater (`FUN_00198b50` → `FUN_00198cc8`) grows a damage volume over
  time (record byte 0 = shape kind; radius and other curves evaluated by `FUN_00197bd8`). Used by Electric,
  Earth, Medusa and `Scr_Concussion`. The blast record layout is open.
- **Direct damage** `FUN_0024bc40(damage, victim move system, …, hit type, …, flags)` (18 callers;
  **Combat_DirectDamage**, HIGH): the same invulnerability and damage-filter path as a weapon hit
  (`docs/combat.md` §3–§4), but without a hit window.
- **Petrify** `FUN_0021c108(amount, victim, source, flag)` (6 callers; **Character_SetPetrify**, HIGH):
  sets the victim state bit `+0x174 |= 0x1000`, posts event `0x15` to its move system, stores the amount at
  `+0x180` and the source at `+0x184`. A victim whose move set has flag `0x80` gets amount 0.

## 5. Electric: `SCR_ElectricCore` (`0x0019d590`). HIGH (code), values RAM

Item id 2, stage rows `GBL+0xf4`, stride 0x5c, WAD `WAD_R_M_Elctrc`. One move (`MOV_ElectricEnter` /
`AirEnter`); the native runs at 0.5 of the move.

1. Checks the meter against the cost (`+0x04`). On success it starts the shared updater (`FUN_0019d390`,
   once), fills a free **core** slot (4 slots of 0x60 bytes at `0x0032fec0`; when all 4 are busy the cast
   does nothing) and pays the cost.
2. **Core** (`FUN_0019cb88`): the effect `goelectriccore` (glow emitters `FXC_LINGemit/1`, `FXC_Flashemit`
   and the sound emitter `SEM_SNDCore`) is placed once at Kratos's joint `+0x08` = **`synchJoint`**. The core
   stays where it was cast (MEDIUM: nothing in the code moves it).
3. **Bolts** (`FUN_0019ce90`, each frame): the targets within `range` (m, × 16 units) of the core
   (`FUN_002126b0`) get up to `max bolts` bolts (5 slots). Each bolt is a `goelectricbolt` model
   (`MDL_electricBolt`) stretched between its rig joints `sourceJoint` (core) and `targetJoint` (target)
   (`FUN_0019c500`). Each bolt hits `rate` times per second (`FUN_0019c790`). Characters get
   `Combat_DirectDamage(damage)` and a knock-back (`Combat_KnockBack`, ground or air impulse), unless they
   block (`MoveSys_GetBlockFlags & mask`), are invulnerable or the global no-damage switch is on.
   Breakable objects get the damage message `0x40f` on server `0x13`.
4. With flag `& 2` one extra bolt cycles through the targets with its own parameters (`+0x3c..+0x52`).
5. The core ends after `lifetime` seconds or when its damage pool is used up. With flag `& 1` it then
   spawns the blast record at `+0x10` (`goelectricexplode`).

| Field | Row 0 | Row 1 | Row 2 |
|---|---|---|---|
| `+0x04` cost | 10 | 12.5 | 16.5 |
| `+0x0c` lifetime (s) | 2 | 2 | 2 |
| `+0x14` damage pool | 200 | 300 | 400 |
| `+0x18` max bolts (s8) | 3 | 4 | 5 |
| `+0x1c` range (m) | 5 | 7.5 | 10 |
| `+0x20` damage per bolt hit | 1 | 1 | 1 |
| `+0x24` hits per second | 4 | 8 | 12 |
| `+0x28` hit type (s8), `+0x2c` block mask | 44, 0x10 | 44, 0x10 | 44, 0x10 |
| `+0x30` ground impulse (3 halves) | 50, 0, 0 | 50, 0, 0 | 50, 0, 0 |
| `+0x36` air impulse | 50, 500, 0 | 50, 400, 0 | 50, 300, 0 |
| `+0x58` flags | 0 | 2 (extra bolt) | 3 (extra bolt, end blast) |

## 6. Earth: six natives. HIGH (code), values RAM

Item id 6, stage rows `GBL+0xf0`, stride 0x68, WAD `WAD_R_M_Earth`. Move chain from the branches:

| Move | Natives (time) | Follow-up |
|---|---|---|
| `MOV_EarthEnter` | `SCR_EarthTrigger` (0), `SCR_EarthEnter` (0.25) | event `0x1c` → `MOV_EarthEcho`, `0x1e` → `MOV_EarthRain`, else `MOV_EarthStomp` at 0.4 |
| `MOV_EarthEcho` | `SCR_EarthEcho` (0.31, 0.61), 3 × `SCR_EarthRockTrigger` per burst | → `MOV_EarthStomp` at 0.625 |
| `MOV_EarthRain` | `SCR_EarthRain` (0), 3 bursts of `SCR_EarthEcho` + rocks | event `0x1d` → `MOV_EarthStomp` |
| `MOV_EarthStomp` | `SCR_EarthStomp` (0.54) | → `MOV_CombatIdle` |

Hashes in the move data (CONFIRMED): `0xa741cd0b` EarthTrigger, `0xdeb9f48b` EarthEnter, `0x301af452`
EarthEcho, `0x287d6b06` EarthRockTrigger, `0x31b0ca41` EarthRain, `0xb8892a84` EarthStomp.

- **`SCR_EarthTrigger`** pays the cost `+0x00` once per cast (a user counter). Then, by the flags `+0x64`,
  it queues event `0x1c` (flag 1: Echo) or `0x1e` (flags 3: Rain).
- **`SCR_EarthEnter` / `Echo` / `Stomp`**: a blast (`Blast_Spawn`) at Kratos's ground joint (controller
  `+0xf4`) from the record at `+0x04` / `+0x08` / `+0x60`, plus the effect `goEarthStomp` (Enter, Stomp) or
  `goEarthRainHit` (Echo).
- **`SCR_EarthRain`**: Circle mash (prompt `PB_CircleBtnSmash`). The charge gains 1 per press, decays by
  `+0x0c`/s and is capped at `+0x10`. The move's animation speed goes from `+0x14` to `+0x18` with the
  charge. The phase ends after `+0x1c` s (charged) or `+0x20` s (not charged), or when `+0x24` rocks were
  thrown, and then posts `0x1d`.
- **`SCR_EarthRockTrigger`**: throws one rock (`goEarthRock`, up to 20 in flight, 0x50-byte slots at
  `0x0032f880`) with a random distance `[+0x2c, +0x28]`, a random `[+0x34, +0x30]`, a random angle
  `[+0x38, +0x3c]`°, `+0x44`, a random `[+0x48, +0x4c]` and `+0x50`. On landing it spawns the blast at
  `+0x54` (effects `+0x58`, `+0x5c`). Field meanings beyond the ranges are MEDIUM.

| Field | Row 0 | Row 1 | Row 2 |
|---|---|---|---|
| `+0x00` cost | 20 | 25 | 30 |
| `+0x64` flags | 0 (Stomp only) | 1 (Echo) | 3 (Rain) |
| `+0x24` rock count (s8) | 10 | 6 | 20 |
| Rain: decay, max, speed, time charged / not | 5, 10, 0–4, 3 / 1 (unused) | 0 (unused) | 1, 20, 0.75–1.25, 3 / 0.5 |
| Rock ranges `+0x28/+0x2c`, `+0x30/+0x34`, `+0x38/+0x3c` | 10/4, 1/0.6, 100/500 | 8/4, 1.25/0.75, 250/750 | 10/5, 1.5/1, 250/750 |

## 7. Wind (held bow `ATT_WindBow`): six natives. HIGH (code), values RAM

Item id 3, stage rows `GBL+0x114`, stride 0xb8, WAD `WAD_R_M_Wind`. **On Pegasus** (character flag
`+0x170` bit 14) all Wind natives use the fixed row at `GBL+0x118` instead.

While the cast button is held, Kratos stays in `MOV_WindIdle`. `SCR_WindTrigger` reads the pad each frame
and posts:

| Input | Mode | Event → move | Needs |
|---|---|---|---|
| Square (bit `0x80`) | 1 | `0x1c` → `MOV_WindGustShot`; held with flag `& 1`: `0x20` → `MOV_WindCharge`, released after `+0x18` s: `0x1d` → `MOV_WindChargeShot` | — |
| Triangle (`0x10`) | 2 | `0x1e` → `MOV_WindTornado` | flag `& 2`, meter ≥ `+0x50` |
| Circle (`0x20`) | 3 | `0x1f` → `MOV_WindTempest` | flag `& 4`, meter ≥ `+0x88` |

`SCR_WindThrow` fires and pays:
- mode 1: one shot from the record `+0x04` (ground) / `+0x08` (air) for the cost `+0x00`, or, when
  charged, `+0x10` projectiles (`FUN_001bfe00`) for the cost `+0x14`;
- mode 2: `+0x48` projectiles `goWindTornado` fanned over `+0x4c`°, cost `+0x50`;
- mode 3: one `goWindTempest`, cost `+0x88`.

The effect names come from the inline descriptors `0x2eb9c0` / `0x2eb9d0` (CONFIRMED). `SCR_WindCharge`
drives the charge timer and slows the animation (`+0x30` = 0.33).

| Field | Row 0 | Row 1 | Row 2 | Pegasus row |
|---|---|---|---|---|
| `+0x00` shot cost | 4 | 4 | 4 | 5 |
| `+0x10` charged shot count, `+0x14` cost, `+0x18` charge time | –, –, – | –, –, – | 6, 25, 1 s | 1 (count), 10, 1 s |
| `+0x48` tornado count, `+0x4c` fan (°), `+0x50` cost | 1, 10, – | 1, 15, 20 | 2, 15, 20 | 1, 10, 15 |
| `+0x88` tempest cost | – | – | 50 | 15 |
| `+0xb6` flags | 0 | 2 (Tornado) | 7 (Charge, Tornado, Tempest) | |

## 8. Medusa (held head `ATT_Medusa`): eight natives. HIGH (code), values RAM

Item id 0x10, stage rows `GBL+0xe8`, stride 0x4c, WAD `WAD_R_M_Medusa`. While the cast button is held,
`SCR_MedusaHeadEvts` posts:

| Input | Event → move | Needs |
|---|---|---|
| Square held | `0x1c` → `MOV_MedusaBeam` (released: `0x1f` → `MOV_MedusaIdle`) | meter > 0.01 |
| Triangle | `0x1d` → `MOV_MedusaFlash` | flag `& 1`, meter ≥ `+0x2c` |
| Circle | `0x1e` → `MOV_MedusaCharge` | flag `& 4`, meter ≥ `+0x34` |

In `MOV_MedusaCharge`, `SCR_MedusaHeadChrgEvts` times the hold. On release it posts `0x1d` →
`MOV_MedusaNuke` when flag `& 2` is set, the meter ≥ `+0x40` and the hold ≥ `+0x3c` s; otherwise it posts
`0x1c` → `MOV_MedusaBomb`.

- **Beam** (`FUN_001a74c8`): drains `+0x14` meter per second while it runs.
- **Flash** (`FUN_001a7b50`, `SND_MEDUSA_RAY_SHOT`): pays `+0x2c`, casts over `+0x18` m and petrifies the
  targets it hits with `Character_SetPetrify(+0x24)`.
- **Bomb**: pays `+0x34`, record `+0x30`. **Nuke**: pays `+0x40`, `Blast_Spawn` with the record `+0x38`.

| Field | Row 0 | Row 1 | Row 2 |
|---|---|---|---|
| `+0x14` beam drain (/s) | 10 | 10 | 10 |
| `+0x18` flash range (m), `+0x24` petrify amount, `+0x2c` cost | – | 12, 250, 25 | 12, 500, 25 |
| `+0x34` bomb cost | – | – | 25 |
| `+0x3c` nuke hold (s), `+0x40` cost | – | – | 0.5, 50 |
| `+0x48` flags | 0 (Beam) | 1 (+Flash) | 7 (+Charge/Bomb, Nuke) |

## Open questions

- ~~`FUN_00216418` (meter spend) and `FUN_00216530` (meter check): confirm that the cost goes to meter 1~~
  (confirmed, §1.1).
- The rest of `Scr_Lightning`: bolt targeting, how the bolts hit (hit windows or direct damage), the
  release callback `FUN_001a5798`.
- ~~The natives of the other abilities (`SCR_Earth*`, `SCR_Wind*`, `SCR_Medusa*`, Electric).~~ Done (§5–§8).
  Still open: the blast record layout (`Blast_Spawn`), the Wind projectiles (`FUN_001bf228`,
  `FUN_001bfe00`), the Medusa beam and bomb internals, the Earth rock flight, the Medusa row fields
  `+0x0c` and the hashes at `+0x10`/`+0x28`.

## 9. The Rust port (2026-10-04)

`gow2-kratos::magic` runs the natives of sections 2 to 8 from the move data, and `gow2-bevy::fx` draws them. How it works, and what is a stand-in:

- **Script actions drive it (HIGH).** The move data's script actions (kind `0x0d`) carry the hash of a script class at `+0x08`; `Dc::names` resolves it (`SCR_Lightning`, `SCR_ElectricCore`, `SCR_EarthTrigger`, `SCR_WindThrow`, `SCR_MedusaHeadFlash`, ...). `MagicSys::update` starts a script object for each action that fires, ties it to the move (it ends with the move, as in the game) and runs the per-frame ones. `gow2-formats` example `script_probe` lists the classes of any move.
- **Per-frame natives:** `SCR_WindTrigger` and `SCR_MedusaHeadEvts` read the pad as in sections 7 and 8 and post the events `0x1c` to `0x20` to the move system (`MoveSys::post_event`), which starts the follow-up moves; `SCR_MedusaHeadChrgEvts` times the Circle hold (nuke or bomb on release); `SCR_MedusaHeadBeam` drains the meter and runs the beam; `SCR_Lightning` counts Circle presses (charge) and strikes; `SCR_EarthRain` ends the rain phase.
- **One-shot natives** (at the moment their action fires): cost payment (`FUN_00216418`), `SCR_ElectricCore` (core slot), `SCR_EarthEnter/Echo/Stomp` and `SCR_WindBlowHit` (blasts), `SCR_EarthRockTrigger` (a rock), `SCR_WindThrow` (gust, charged gusts, tornado, tempest), `SCR_MedusaHeadFlash` (petrify in a cone), `SCR_MedusaHeadBomb`, `SCR_MedusaHeadNuke`.
- **Stage tables** (section 5 to 8) are in the module for upgrade rows 0 to 2; `GOW_MAGIC_LEVEL` picks the row (default 2). The entry check uses the cost of the selected magic (`cast_cost`).
- **Stand-ins (LOW)**, all named constants in `magic.rs`: the blast records (radius, damage, push of every blast; the layout is not decoded), the damage and speed of the wind shots, the tornado and tempest sizes, the beam's reach, width, damage and petrify time, the bomb's flight, the rock's flight and range, how lightning bolts are aimed. Petrified victims (new `Victim::petrify`) stand still, are drawn dark grey and shatter at the next hit.
- **Aim (HIGH for the formula).** `SCR_AimDownUp` (`FUN_00250ea0`, `FUN_00250c38`, `FUN_002509c8`) takes the angle from the character's aim joint to the target above the horizontal, clamps it to the action's range (`+0x18`, `+0x1c`: -30 to +60 degrees on the ground, -45 to +45 in the air) and maps it to -1 .. +1 as `2 * (angle - min) / (max - min) - 1`; with no target the direction is level. The three aim clips of a group (`...00`, `...01`, `...02`) are laid over a full-body stance (`magMedusaStrafeIdle`, `magWindStrafeIdle`) with weights of that value (the aim clips animate only the spine, head and arms: `Hero::pose_over`). The port has no target system and aims at the nearest live dummy within 30 m (LOW).
- **Objects held in the hand.** `ATT_Medusa` (right hand: `RWeapIH` hand slot, `RWeapOH` free slot, snap 2 m) and `ATT_WindBow` (left hand, snap 1 m); `ATT_Bone`, `ATT_Hammer` and `ATT_Olympus` use `RWeapIH` for both slots with snap 0 (`att_probe` example). The head is drawn at the hand joint's position with the free joint's rotation (the free joint is the one with the body's own axes, so the head's face, which looks along its -z as the beam model does, looks along the arm; MEDIUM). The bow is placed the same way on the left.
