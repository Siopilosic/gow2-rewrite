# Kratos: controller, character object and gameplay data

_Phase 3, first stage toward `project-goal.md` (player/Kratos). It started from the runtime chain in
`runtime-validation.md` (`bp-hero-pos`). Confidence levels as in `hypotheses.md`._

## 1. Character class (vtable 0x002f1440)

**Hierarchy.** Vptrs are stored at **+4**. This is a game-side hierarchy, separate from the engine `PolyObject` (vptr at +0x20).

```
0x2f17b8  root (ctor 0x00218ed8; slots 2, 12-16 = __pure_virtual)
 ├─ 0x2f1578  abstract (ctor 0x0021cc38 = 0x218ed8 + vptr; slot 2 still pure)
 │   ├─ 0x2f1440  "Character"  ctor 0x00224af8   <- Kratos, Rhodes soldiers, Colossus (RAM)
 │   ├─ 0x2f0d08  ctor 0x0022de68
 │   ├─ 0x2f0998  ctor 0x00254b60 (slot 2 0x002564a0 also calls Node_SetMatrix)
 │   └─ 0x2f0fe0, 0x2f11a8 (installer not found by the scanner)
 └─ 0x2f0708 (0x00257ce8), 0x2f0850 (0x00259620)
```
Source: `analysis/vtables.tsv`, by shared slots (0x21beb0, 0x284ae8, 0x284b10, 0x284b20) and the ctor chain. CONFIRMED (static).

**Slots of 0x2f1440.**
- Slot 1 is `0x00224c30` (dtor, 344 bytes).
- Slot 2 is `Character_Update` `0x00228f90` (11,012 bytes, 421 callees).
- The full list is in `analysis/vtables.tsv`. The large own slots are 3 (`0x224018`), 29 (`0x224e58`) and 34 (`0x22c2c8`).

**`Character_ctor` `0x00224af8(this, spawner, crt, anim?, is_player)`.**
- Calls the base ctor, then sets the vptr.
- Clears `+0x37c` and `+0x3b0..+0x3b8`.
- If `is_player != 0`, sets `+0x174 |= 0x200000`.
- Calls `FUN_002247c8(this, crt, anim?)`.

**Fields**

| Offset | Meaning | Evidence | Confidence |
|---|---|---|---|
| `+0` | GO node (`gohero` 0x006fd480 for Kratos) | RAM | CONFIRMED |
| `+4` | vptr | ctor + RAM | CONFIRMED |
| `+0xc` | WAD context of the owning `CXT_R_*` | RAM: hero → `CXT_R_Hero`'s WAD, soldiers → `CXT_R_Rhsold00`'s | HIGH |
| `+0x174` | flags; bit **0x200000 = player** | ctor (only the hero path passes 1) + RAM (hero 0x00200000; two enemies 0) | CONFIRMED |
| `+0x1b8` | back pointer to the owning controller's data (`controller+0x10`) | creators + RAM | CONFIRMED |
| `+0x288` | 0x500000 for the hero, 0xa00000 for Enemy1 | creators | MEDIUM (meaning unknown) |

## 2. Controller types (`analysis/controller_types.tsv`, `tools/controller_types.py`)

A controller owns a Character (at `controller+0x10`). Its gcc vtable has a fixed shape:
- slot 1: dtor;
- slot 2: create(controller, spawner);
- slot 5: factory, which allocates from the current WAD heap and constructs the controller;
- slot 6: returns the type name;
- slot 8: returns a constant (1 for Player);
- slot 9: returns `hash(hfsm name)` via `FUN_00181428`.

The first `tools/vtables.py` scan missed these vtables, because their slots 3/4 are null.

| vtable | type | HFSM | create (slot 2) |
|---|---|---|---|
| 0x002effc8 | **Player** | hfsmPlayer | 0x0026a3a0 → `Hero_Create` 0x0020bb08 |
| 0x002eff70 | **Enemy1** | hfsmEnemy1 | 0x0026a730 → 0x001ff3b8 |
| 0x002eff18 | PegasusEnemy | hfsmPegasusEnemy | 0x0026af58 |
| 0x002f0078 | Pegasus | hfsmPegasus | 0x00269e60 |
| 0x002f0020 | Reactive | hfsmReactive | 0x0026a0f8 |
| 0x002efdb8 | Breakable | hfsmBreakable | 0x0026ba38 |
| 0x002efd08 | HeroBreak | hfsmHeroBreak | 0x00270a00 |
| 0x002efd60 | SavePoint | hfsmSavePoint | 0x0026ca30 |
| 0x002efcb0 | ArrowEmitter | hfsmArrowEmitter | 0x00270b58 |
| 0x002f1730 | BalanceGeom | hfsmBalanceGeom | 0x00193008 |
| 0x002efe10/68/c0 | IO_CSM / IO_TandF / IO_Misc | — | |

**Runtime check (`ingame2`).** There is exactly one object with vptr 0x2effc8 (0x00958650, the hero controller; the hero Character's `+0x1b8` = `0x00958650+0x10`). There are 5 objects with vptr 0x2eff70 (soldiers). CONFIRMED.

**Consequence.** All ordinary enemies share the one **Enemy1** controller. Per-enemy differences must come from data (`CRT_*`, below). HIGH.

## 3. `Hero_Create` 0x0020bb08 (static)

Strings: `"CRT_Hero"` (0x2ee318), `"goTargetLK"` (0x2ee328), `"MAT_swordtrail"`.

What it does:
- Sets `node+0xf4 |= 1` on the spawner's node.
- Builds the Character with `crt = FUN_001204c0("CRT_Hero")` and `is_player = 1`.
- Sets `Character+0x1b8` to the controller.
- Sets a value from global `0x2d870c` into `Character[0x5f]`.
- Calls `FUN_00223df0(1.0, 10.0, ch, 0x6bfffb)`.
- Sets `+0x288 = 0x500000`.
- Looks up the node named `goTargetLK`.
- Creates a sword-trail effect object (`MAT_swordtrail`, `FUN_00139900`).

`FUN_001204c0(name, 0)` resolves a `CRT_*` name. It lives in the 0x1204xx range, next to the DC tag handlers (below). MEDIUM.

## 4. Hero gameplay data on the disc: `DC_*` records (WAD tags 0x0b–0x10)

**Where.** Every `R_HERO*.WAD` (10 variants) has a `DC_WAD_R_Hero` record for each of tags 0x0b–0x10.
- The handlers are `0x00120588`, `0x001205e8`, `0x00120698`, `0x001207b8`, `0x00120898` and `0x001208b8`. They are adjacent to `FUN_001204c0` and in the same code region as EvtServer (`0x00121788` ctor, `0x001218d8` update). EvtServer is the server whose update runs Kratos (`runtime-validation.md`).
- The strings below were extracted with `tools/dc_strings.py` into `analysis/dc/R_HERO00/`. Their **presence is CONFIRMED** (disc). The **interpretations are MEDIUM/LOW** until the parsers are traced.

> **Superseded by §6 (pass "tag 0x10 decoding", same day).** The loaders show that 0x0d and 0x0e are the export and import symbol tables of the 0x0c blob. 0x0f and 0x10 are **debug tables that the game ignores** (a hash→name table and an object index). The combat data is the 0x0c blob. The "Interpretation" column below is kept as the first-pass record.

| Tag | Size (R_HERO00) | Content (string evidence) | Interpretation |
|---|---|---|---|
| 0x0b | 4 | — | header/count? |
| 0x0c | 380,960 | few strings, mostly binary | bulk data (LOW) |
| 0x0d | 2,528 | `CRT_Hero`; `ATT_Chains`, `ATT_Bone`, `ATT_Hammer`, `ATT_Medusa`, `ATT_Olympus`, `ATT_WindBow`; `NB_DefaultBank`, `NB_WindBank`, `NB_BoneBank`, `NB_HammerBank`, `NB_MedusaBank`, `NB_OlympusBank`; 86 `BRA_*` (`BRA_HitAir`, `BRA_FatMedParry`, `BRA_CombatIdle`, `BRA_HazardFront`, …); `TWRP_*` | name table: creature, weapon/magic **attachments**, one **move bank per weapon/magic**, reaction branches (MEDIUM) |
| 0x0e | 12,452 | 290 `CSH_*` (camera shake TINY…GIANT), 315 `FFB_*` (force feedback), 44 `TPR_`, 17 `MFX_` | combat feedback presets (MEDIUM) |
| 0x0f | 57,844 | **`hfsmPlayer`**; 982 `MOV_*` (e.g. `MOV_BasicTriangle01/02`, `MOV_OlympusTriangle01..04`, `MOV_AirBoneTriangle01`, `MOV_HammerBlock`, `MOV_BoneBlock`, `MOV_AirBlock`, `MOV_MedusaFlash`, `MOV_WindTornado`, `MOV_TitanParry`, `MOV_CombatIdleStep`, `MOV_HitAir`, `MOV_WallSlideEnter`); `SND_`, `SCR_` | HFSM + move name table (MEDIUM) |
| 0x10 | 415,232 | typed objects: `tSoldier_97` (for `CRT_Hero`), `tChained_101/102` (`ATT_Chains`), `tHand_104..112` (other ATTs), `tCameraTarget_99`; 1,041 `MOV_*`, 3,761 `tBranch_*`, and actions `tActionSetFlags` 1,186, `tActionScript` 1,040, `tActionSound` 993, `tActionPlayFX` 571, `tActionFootEffect` 557, `tActionMeterAdjust` 353, `tActionForceFeedback` 315, `tActionCameraShake` 290, `tActionHitCounterPause` 242, `tActionCameraObject` 211; `TweenIn`/`TweenOut`; `PFX_*`; parameters `PushableMinSpeed`, `PushKickMaxSpeed` | a serialized object graph of **moves, branches (move→move transitions = combos/cancels) and per-move timed actions** (sound, FX, camera shake, rumble, hit-stop, meter changes). This is the core combat data (MEDIUM) |

**Observations**

- The hero's creature type in the data is **`tSoldier`**, while the enemies' controller is named `Enemy1`. "Soldier" may be the shared combatant class in data. LOW.
- `ATT_Chains` uses `tChained`, while the other weapons and magics use `tHand`. This is consistent with chained blades vs hand-held items. MEDIUM.

**Mapping to GoW II names (project-goal arsenal), not yet proven.** The item-id ↔ data-name mapping is now code-confirmed (§6.7). The **retail item** names below are still unproven.

| Data name | Likely GoW II item | Confidence |
|---|---|---|
| `Chains` / `Basic` | Blades | MEDIUM |
| `Olympus` | Blade of Olympus | MEDIUM |
| `Hammer` | Barbarian Hammer | MEDIUM |
| `Medusa` | Head of Euryale | MEDIUM |
| `WindBow` / `Wind` | Typhon's Bane | MEDIUM |
| `Bone` | Spear of Destiny? | **LOW** |
| (no data name yet) | Cronos' Rage and Atlas Quake | — |

The level contexts `CXT_R_M_Earth`, `R_M_Medusa`, `R_M_Wind` and `R_M_Lghtn` (`runtime-validation.md`) show the magics. Atlas Quake (`M_Earth`) and Cronos' Rage (`M_Lghtn`) are not among the `ATT_*` entries. They may be data without a held attachment.

## 6. Pass "tag 0x10 decoding" (2026-10-02): the DC format and the move system

**Sources for this pass**
- Static: `FUN_001204c0/f0/88`, the six DC handlers, `FUN_00181428` (hash), `FUN_00247bd8` (action executor), `FUN_0024f250` / `FUN_0024e210` / `FUN_0024eb00` / `FUN_0024f1e8` / `FUN_00250010` (branches), `FUN_0024ea30`, `FUN_0024e8d0`, `FUN_00210690`, `FUN_00210728`, `FUN_0022c250`.
- Disc: every DC object of `R_HERO00` and `R_PERMA`.
- Runtime (no new captures): the existing `ingame1`/`ingame2` RAM images.
- Saved decompilation: `analysis/core_decomp/FUN_00247bd8_action_execute.c`, `FUN_0024f250_branch.c`, `FUN_0024e210.c`, `FUN_0024eb00.c`, `FUN_0024f1e8.c`, `FUN_00250010.c`.
- Tools: `tools/dcparse.py` (tables) and `tools/dcmoves.py` (move graph), output in `analysis/dc/<WAD>/`.

### 6.1 Correction: what tag 0x10 is

| Tag | Handler | What it does | Confidence |
|---|---|---|---|
| 0x0b | 0x00120588 | `FUN_00120ab8(curWAD->+0x48, hdr->name)`: creates a named **DC container** in the current WAD; payload is 4 bytes | CONFIRMED (code) |
| 0x0c | 0x001205e8 | finds the container; if `param & 0x2000` the payload stays in place (`param |= 0x5000`), otherwise it is copied to a heap block (`FUN_0014b328(size, 16)`). Container `+4` = **blob** | CONFIRMED (code) |
| 0x0d | 0x00120698 | **exports**: `u32 n; {u32 blob_off, u32 name_off}[n]`. For each, registers `"%s_DC"` → `blob + blob_off` in the current dictionary (`Dict_Replace`/`Dict_Insert`) | CONFIRMED (code); 106 exports parsed |
| 0x0e | 0x001207b8 | **imports**: same layout. For each, `t = FUN_001204c0(name, 0)`; if found, `*(blob+off) = t - (blob+off)`, a self-relative pointer | CONFIRMED (code); 667 parsed. RAM blob differs from the disc copy in exactly 667 words (`ingame1`/`2`) |
| 0x0f | 0x00120898 | **nothing**: only `if (param & 0x2000) param |= 0x1000` | CONFIRMED (code) |
| 0x10 | 0x001208b8 | **nothing** (same body) | CONFIRMED (code) |

**Debug tables the game ignores.** Their layouts were established from the disc:
- **0x0f** = `u32 n; {u32 hash, u32 name_off}[n]`. All 2,310 (hero) and 527 (perm) entries satisfy `hash == FUN_00181428(name)`. CONFIRMED (disc + hash).
- **0x10** = `u32 n; {u32 blob_off, u32 name_off, u32 type_id}[n]`.
  - 13,694 hero objects, all offsets inside the blob, 80 type ids.
  - Evidence: offset 0 = `CRT_Hero` = export 0; the type ids are consistent per name prefix; the per-type layouts are confirmed by code below.
  - Status: layout CONFIRMED (disc); the **type ids are author-side** (the game never reads them).

**Consequence.** Tag 0x10 is not the combat data. It is a debug **object index** that labels every object of the 0x0c blob with a name and type. That is why the blob could be decoded statically.

**Lookup.** `FUN_001204c0(name, scope)` = `FUN_001204f0(hash(name), scope)`, which computes `h = FUN_00181428("_DC", hash(name))` (hash continuation). Then:
- scope 0: `FUN_001872a0(h)`, the global dictionary;
- otherwise: `FUN_0018de98(scope, h)`.

CONFIRMED (code).

**Hash.** `FUN_00181428(s, seed)`: `h = seed; for c in s: h = h*127 + toupper(c)` (32-bit, signed `lb`). CONFIRMED (code; reproduced in `tools/dcparse.py`, 2,837/2,837 table hashes match).

### 6.2 Blob conventions (CONFIRMED)

- **Self-relative pointers.** Target = field address + `s32` value; 0 = null.
  - Code idiom: `lw v; addu p, field, v; movz p, zero, v` (e.g. `FUN_0022c250`).
  - Data: every `tBranch+0` and `ATT`/`CRT` reference resolves to an indexed object.
- **Packed lists.** One `u32` with `count = w & 0xfff` and `array = field + (s32 w >> 12)`; the array holds self-relative pointers.
  - Code: `FUN_00247bd8` (`andi 0xfff`, `sra 0xc`), `FUN_0024f250`, `FUN_00219070`.
  - Data: all 1,041 hero moves resolve.
- **Time windows** are IEEE half-floats in normalized animation time [0, 1].
  - The code converts them inline: `((h & 0x8000) << 16 | (h & 0x7fff) << 13) / 2^-112`.
  - Normalized time = `animplayer+8 / clip+0x14`, clamped to [0, 1].
- **Read-only at runtime.** Apart from import patching, the blob is not modified (RAM == disc for all other 94,573 words, captured in two different game states). CONFIRMED.

### 6.3 `MOV` (type id 0x62, 1,041 in `R_HERO00`)

| Offset | Type | Meaning | Evidence |
|---|---|---|---|
| +0x00 | u32 | ? (e.g. 0x30cc3c00; looks like two halfs) | — UNKNOWN |
| +0x04 | u32 | ? (0x700/0x701/0x702/0x300) | — UNKNOWN |
| +0x08 | u32 | **hash of the animation clip name** (`navIdle`, `navCombatIdle`, `attBrutalSlam`) | CONFIRMED (disc: 1,041/1,041 hashes are in the 0x0f table). Runtime: `MOV_Stand` → `navIdle` while the anim player ran a 2.0 s clip. The consumer of +0x08 is not traced yet, so "clip" is HIGH |
| +0x0c | u32 | hash of the move's own name | CONFIRMED (disc 1,041/1,041) |
| +0x10 | packed | **branches** (3,554; `tBranch` 0x5f and `BRA` 0x60) | CONFIRMED (`FUN_0024f250` + disc) |
| +0x14 | packed | **`tCollision` list** (174) | CONFIRMED that it is a list (`FUN_00247bd8`, `FUN_00232128`, `FUN_00247968`, `FUN_0024fef0` and `FUN_00250590` unpack `+0x14`; disc: all targets are type 0x61). ~~The `tCollision` content is not decoded (4-byte objects, `+0` is not a pointer)~~ Decoded 2026-10-03: they are hit windows (window, ground/air/block impulse, damage, volume, flags, sub-hits); see docs/combat.md §1 |
| +0x18 | packed | **actions** (6,974 = exactly the number of `tAction*` names) | CONFIRMED (`FUN_00247bd8` + disc) |
| +0x1c… | | the arrays of the lists usually follow here | data |

### 6.4 `tBranch` (type 0x5f, 40–64 bytes) and how a transition is chosen

**Per-frame evaluator** (`FUN_0024f250(time, moveSys, input)`). CONFIRMED (code).
1. For each branch `b` of the current move (`moveSys+0x6c` → `MOV+0x10`): if the normalized time is in `[half(b+0x10), half(b+0x12)]` and `FUN_0024e210` (input) and `FUN_0024eb00` (state) both pass, it computes `score = FUN_0024f1e8(b)` and keeps the set of best-scoring branches.
2. Otherwise it clears the branch's bit in `moveSys+0x18`.
3. A branch already queued at `moveSys+0x2bd` competes as well.
4. One winner is picked **at random** among equal scores (`FUN_0014be00`).
5. The winner is queued via `FUN_00250010(t, moveSys, b)` into a ring buffer: entries `{branch, time}` at `moveSys+0x2b8`, capacity byte `+0x2bc`, head `+0x2bd` (0xff = empty), tail `+0x2be`.
6. If `flagsA & 0x400` is clear, the queued time is the window end, i.e. the transition **waits for the end of the window** (buffered input). With 0x400 it happens at once.

**Runtime.** Kratos's move system is at `0x00957370`:
- `+0x2bc` = 8 (capacity), `+0x2bd` = 0xff (empty), `+0x2be` = 0;
- `+0x6c` = `MOV_Stand`;
- `+0` → context `0x00957230`, whose `+0x30` = Character `0x00958160`;
- `+4` → anim player `0x00ba3428`.

CONFIRMED.

**Score** (`FUN_0024f1e8`). CONFIRMED (code).
- `flagsA & 0x800` → 5; `flagsA & 0x400` → 4;
- otherwise: button `0x0b` → 1; press-mode 3 → 2 if already buffered, else excluded; any other → 3.

**Fields**

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| +0x00 | rel | **target `MOV`** (3,387 to MOV, 623 null) | CONFIRMED that it points to a MOV (disc). ~~That it is the transition target: HIGH (the consumer of the queued branch is not traced yet)~~ The transition target: CONFIRMED (code, `MoveSys_TakeBranch` §6.4a) |
| +0x04 | rel | compared with a context object passed by the caller (`FUN_0024eb00` param 11). Null in all 4,010 hero branches | CONFIRMED (code); meaning UNKNOWN |
| +0x08 | u32 | **flagsA**, see the flagsA list below | CONFIRMED (code) |
| +0x0c | u32 | **flagsB**, see the flagsB list below | CONFIRMED (code) |
| +0x10 / +0x12 | half | **input window** start / end in normalized time | CONFIRMED (code) |
| +0x14 | u16 | ? | UNKNOWN |
| +0x16 / +0x18 | s16 | allowed range of the **target's** `+0x178` (`FUN_0024e030`, -1 if none) | CONFIRMED (code). `+0x178` = health: HIGH (§6.8) |
| +0x1a / +0x1c | s16 | allowed range of the **own** Character's `+0x178` | CONFIRMED (code) |
| +0x1e | s8 | compared with `FUN_0024dfe8()` (from the target's `+0x1c8` object; −1 = any) | CONFIRMED (code); meaning UNKNOWN |
| +0x1f | s8 | compared with `FUN_0024df70()` (a target class value from the target's `+0x1c8` list or `+0x44`→`+0xd`; −1 = any); `flagsA & 0x1000` allows a mismatch | CONFIRMED (code); meaning MEDIUM (enemy class / size for grabs; 148 branches use 0x24) |
| +0x20 | u8 | **button / trigger code**: 1–10 map through table `0x002f6da7` to a pad bit (see the input-code list below); other values are special triggers | CONFIRMED (code) |
| +0x21 | u8 | **press mode**: 1 = just pressed (`cur & (cur ^ prev)`), 2 = just released, 3/4 = held, 5 = not held | CONFIRMED (code: input `+0x28` = current mask, `+0x2c` = previous) |
| +0x22 | u8 | **stick condition** (see the input-code list below) | CONFIRMED (code) |
| +0x23 | u8 | **unlock / selection requirement** (`FUN_0024e8d0`; see the input-code list below) | CONFIRMED (code + disc consistency, §6.7) |
| +0x24 | s8 | **minimum level**: compared with base level byte `0x00335869`, or with the level of the selected item (`FUN_00210690(0x00335874)`) when `g_0x36a81c & 0x7c00` | CONFIRMED (code); 89 hero branches use 1–4 |

**flagsA (`+0x08`)**
- Low bits must intersect the character-state mask from `FUN_0024e068`, derived from Character `+0x170`/`+0x174`. The bits come out as 1/2/4/8/0x10/0x20/0x40/0x80/0x100. Their exact state names are UNKNOWN; 1 vs 2 separate ground and air moves (all `*AirEnter` magic branches use 2), so that pairing is HIGH.
- 0x200: requires `moveSys+0x2b4 & 2`.
- 0x400: immediate (no wait).
- 0x800: highest priority.
- 0x1000: allows a `+0x1f` mismatch.
- 0x2000: inverts the `+0x23` requirement.
- 0x4000: requires a valid target handle.

**flagsB (`+0x0c`)**
- 0x01/0x02: target `+0x1e0` (present / absent).
- 0x04/0x08: target `+0x170` bit 13.
- 0x10/0x20: global `0x36a81c & 0x200`.
- 0x40/0x80: own `+0x174 & 0x400000`.
- **0x100…0x2000: sub-weapon gating** (`FUN_0024ea30`, see §6.7).
- 0x4000/0x8000 and 0x10000/0x20000: bits 8 / 4 of `FUN_001b19b0()+0x6ec`.

**Input codes (`+0x20`, `+0x22`, `+0x23`)**
- `+0x20` button codes 1–10 map to pad bits 6, 7, 4, 5, 2, 0, 9, 3, 1, 10. Which physical buttons those bits are: see §6.6.
- `+0x22` stick conditions:
  - 1: stick magnitude < 0.8; 2: ≥ 0.8;
  - 3–6: direction quadrant (`FUN_002166d8`);
  - 7/8: dot with the facing ≥ 0.707 / ≤ −0.707;
  - 9/10: the same on the perpendicular;
  - 0x0b–0x14: the same tests on the **right stick** (`+0xf0` vs `+0xe0`), only when `*(0x2d8dcc+4) == 1`;
  - 0x15–0x17: speed thresholds from `GBL_Global+0x24/0x28/0x2c`.
- `+0x23` requirement codes:
  - 0: none;
  - 5/10/0x0b: unlock bit `(code − 1)` of `0x00335834`;
  - 7: `0x00335834 & 0x40`;
  - any other code: the selected magic `0x00335873` must equal the code, and `FUN_00210728` must accept it (§6.7).

**Example (disc; `analysis/dc/R_HERO00/branches.tsv`).** `MOV_BasicSquare01`:
- button 2 just pressed in [0, 0.275] → `MOV_BasicSquare02` (buffered);
- the same in [0.275, 1] with 0x400 (immediate);
- button 3 → `MOV_BasicTriangle01`;
- button 5 held → `MOV_Parry`, or `MOV_GoldenParry` if unlock 5;
- button 4 → `MOV_CombatThrow`;
- button 2 held in [0.175, 0.275] with min level 3 → `MOV_CrushTransition`.

That move→move-on-button-in-window structure is **what the game evaluates**. Calling these chains "combos" is a description of that behaviour. HIGH (code); a runtime check of a queued branch is still pending.

### 6.4a Taking a queued branch (the dequeue side). HIGH (code, pass7, 2026-10-03)

This closes the open item "who switches `moveSys+0x6c`" and confirms that branch `+0x00` is the target.

**`FUN_0024cb78(moveSys)` MoveSys_Update** (8 callers, incl. the end of `Character_Update`):
1. Unless the owner rides a host (`+0x170 & 0x2000`), run each active move instance's actions once this
   frame and advance it (`FUN_00249560`, which also evaluates its branches, §6.4).
2. For each move instance whose flag `+0x2b4 & 4` is set (the queued transition is due; the flag is
   cleared): if its branch queue is empty (`+0x2bd == 0xff`) the move ends (`FUN_002504f8`); otherwise
   pop `{branch, time}` from the ring buffer (`+0x2b8`, capacity `+0x2bc`, head `+0x2bd`, tail `+0x2be`)
   and **take the branch**: `FUN_002501d0(-1.0, moveSys, move, branch)`.
   - Branches with button code `0x1a`/`0x17` (and `0x1b`/`0x17`/`0x19` under flag `0x4000`) bump two
     counters (`+0x30`, `+0x34`) of the object from the owner's virtual `+0x64`. MEDIUM: input statistics.
3. With no move pending and the owner on the ground (not `+0x170 & 2`, not `+0x174 & 0x800000`),
   `FUN_0024f6c0` evaluates the locomotion moveset, then a second ring buffer at `moveSys+0xb4`
   (capacity `+0xb8`, head `+0xb9`, tail `+0xba`) supplies **moves requested by code**. They start only
   when every active move allows it (`(move+0x6c)->+4 & 8`).
4. The gravity scale `+0x34` moves 10 % per frame toward `+0x38`; the timer `+0x3c` counts down by the
   node's frame time (`FUN_0013c408`) and resets `+0x38` to 1.0 when it expires. `+0x68` follows
   `+0x44 → +0x3c` the same way. Then `FUN_0024d7b8`, the time-in-move accumulator `+0x54`, the
   `0x200000`/`0x800000` move-flag handling (`FUN_0024c660`), and `FUN_0024c4b0` / `FUN_0024c5e0` from
   the combined move flags (bits 1, 3, 5 and 2, 4, 6; meaning open).

**`FUN_002501d0(time, moveSys, from, branch)` MoveSys_TakeBranch:**
- flagsA `0x20000`: pick a substitute branch first (`FUN_00250120`); `0x10000`: do nothing; `0x8000`: only
  end the current move;
- otherwise the **target `MOV` = branch `+0x00`** (self-relative). Its clip index is found from the anim
  hash `MOV+0x08` in the owner's clip table (`FUN_0023b0f8`); if the clip is missing nothing happens;
- unless flagsA `0x40000` is set or the target has `MOV+4 & 8` (a move that layers over the current
  one), the current move ends (`FUN_002504f8`, or `FUN_00250490` for all);
- clips with header flags `0x108` end any move using the same clip (`FUN_00250528`);
- a new move instance is taken from the owner WAD's pool (`FUN_00246fa8`) and started.

**`FUN_002470b8(time, instance, moveSys, branch, clip)` Move_Start:**
- clears the instance: hit list (48 triples, §combat), branch queue (capacity 8 at `+0x2c0`), action
  bits; links it into `moveSys+0xa8`; `+0x68` = branch, `+0x6c` = `MOV`;
- **start time** = `time`, or when it is −1.0 (the normal case) the half at **branch `+0x14`**: the
  normalised time at which the target move starts. Hero data: 0.0 in 3,616 branches, 0.05 in 186, 0.1 in
  115, up to 0.45 (`analysis/dc/R_HERO00/branches.tsv` column `start_time`);
- starts the clip: `FUN_0023b310(half(MOV+0) · ctx+4, half(MOV+2) · ctx+8, start, anim system, clip)`.
  In the hero data `MOV+0` is mostly 1.0 and `MOV+2` 0.1 or 0.2: MEDIUM that these are the playback
  rate and the blend-in time in seconds;
- a clip without flags `0x108` becomes the move system's main move (`moveSys+0xb0`);
- runs the new move's actions at once; posts event `0x40b` with the move name hash;
- for the player, records the move with `FUN_00216188(0x335880, MOV)` unless `MOV+4 & 8` or the move
  flags `0xe600000` (MEDIUM: last-move/combo record);
- with move flag `0x200000`, passes the same branch to a linked character (`FUN_0024ffe8`): the partner of
  a synchronised move (grabs, the ridden mount when `+0x170 & 0x4000`).

### 6.5 Actions (`MOV+0x18` list; executor `FUN_00247bd8`, ~80 callers incl. 9 in `Character_Update`)

**Executor loop.** CONFIRMED (code).
- Common header: byte `+0` = **kind**, `+1` = flags, `+2` = **trigger**, `+3` = s8 **minimum level**, `+4/+6` = half **window**.
- Persistent effects are kept in a list at `moveSys+0x48` (`param_1[0x12]`) and expire when the time leaves their window.
- Per action:
  1. If the time is outside the window, skip it.
  2. If its bit in `moveSys+0x38` is set (already fired), skip it.
  3. Evaluate the trigger:
     - **0**: always (sets the fired bit, so it fires once per window);
     - **1/3/5**: fire when `moveSys[0x17]/[0x16]/[0x18] + 1 == g_frame@0x003630d8`, i.e. an event that happened this frame;
     - 2/4/6: other event tests.
  4. Check the min level against the same level source as branches.
  5. Dispatch through the jump table `0x002eeeb0` on `kind − 1` (32 entries).

**Trigger 1 = "on hit".** ~~MEDIUM.~~ CONFIRMED (code) 2026-10-03: the combat code stamps hit (1), kill (3) and blocked (5) frames on the attacker's move instances; see docs/combat.md §6. Every camera-shake, rumble, meter and `SoundOnEnemy` action of the attack moves uses trigger 1, e.g. `MOV_BasicSquare01` actions 3–5 and 10. Confirming it needs a write watch on `moveSys+0x5c`.

**Kind codes.** CONFIRMED (disc: one kind per type id, no exceptions; code: jump table and case bodies).

| Kind | Object type (debug name) | Count (hero) | Handler / known semantics |
|---|---|---|---|
| 0x01 | tActionSetFlags | 1,186 | `moveSys[0xac] |= a+8`. 0x2000000 → `moveSys[4]|=1`, 0x4000000 → `|=2`, 0x400 → `moveSys[5]|=1`. CONFIRMED (code); the flag meanings are UNKNOWN |
| 0x02 | tActionMeterAdjust | 353 | amount `f32 a+8`. Selector `a+0xc`: 0 → own `+0x178` (Character `puVar1[0x5e]`), 1 → `g_0x335840`, 2 → `g_0x335848`. Flags `a+0xd` (bit 2 = relative to the meter value). CONFIRMED (code) |
| 0x03 | tActionDampingAdjust | 3 | case at 0x002487f0 |
| 0x04 | tActionArrowEmitter | 1 | case at 0x002480cc |
| 0x07 | tActionSound | 993 | `a+8` = sound name hash (in 0x0f). Case 0x002481e8 |
| 0x08 | tActionSoundOnEnemy | 73 | case 0x002482a8 |
| 0x09 | tActionSoundWindow | 24 | case 0x0024830c |
| 0x0a | tActionCameraShake | 290 | `a+8` = import → `CSH_*` (R_PERMA). Case 0x002483c8 |
| 0x0b | tActionForceFeedback | 315 | `a+8` = import → `FFB_*`. Case 0x002483f4 |
| 0x0c | tActionSlowdown | 23 | case 0x00248420 |
| **0x0d** | **25 different debug types** (Script, PlayFX, CameraObject, ClearRadius, ForceAttachment, SubWeaponToggle, HammerSoul, Concussion, TimedPress, RepeatPress, Zoom, Shield, …) | 2,700+ | one case (0x0024847c). These are a second-level, object-dispatched family; the sub-dispatch is not yet traced |
| 0x0e | tActionOverrideDamage | 11 | 0x002488bc |
| 0x0f | tActionVelocityScale | 76 | 0x002488f0 |
| 0x10 | tActionWindImpulse | 15 | 0x00248948 |
| 0x11 | tActionTimeWarp | 9 | 0x00248aa8 |
| 0x12 | tActionContext | 101 | 0x00248ad4 |
| 0x13 | tActionBlock | 21 | 0x00248ae8 |
| 0x15 | tActionHitPause | 79 | 0x00248b18 (hit-stop) |
| 0x17 | tActionHitCounterPause | 242 | 0x00248b50 |
| 0x18 | tActionDeathMenuPause | 17 | 0x00248b70 |
| 0x19 | tActionNoBlock (perm) | 6 | 0x00248b90 |
| 0x1b | tActionTimer | 8 | 0x00248be0 |
| 0x1d | tActionFootEffect | 557 | 0x00248cd4 |
| 0x1f | tActionCliffEffect | 109 | 0x00248f44 |

~~The case bodies beyond SetFlags/MeterAdjust are **not decoded yet**.~~ Decoded 2026-10-03 for HitPause, Slowdown, VelocityScale, CameraShake, ForceFeedback, TimeWarp, OverrideDamage, Context, Block, HitCounterPause, DeathMenuPause, Timer: see docs/combat.md §7–8. The names in column 2 come from the debug index, so the semantics in that column are LOW unless stated.

### 6.6 Inputs

- **Button codes** come from move names (MEDIUM; not yet confirmed by pad code or runtime). Over all branches, the target-move names group by code:

  | Code | Target moves | Likely button |
  |---|---|---|
  | 1 | `SpecialCross`, jumps, evades | Cross |
  | 2 | `*Square01` | Square |
  | 3 | `*Triangle01`, `HeavyAttack` | Triangle |
  | 4 | `CombatGrapple`, `CombatThrow` | Circle |
  | 5 | block / parry / `Air*Special`, held | block button |
  | 6 | **magic casts** (`*Enter` of Lightning/Electric/Earth/Wind/Medusa) | |
  | 8 | `AmuletEnter` | |

  The physical pad bits (6, 7, 4, 5, 2, 0, 3) need the pad-mask builder or a runtime test (hold a button, read the input struct `+0x28`).
  Update 2026-10-03: bit 5 (`0x20`) = **Circle** is HIGH: the Lightning and Earth natives show the `PB_CircleBtnSmash` prompt and count presses of `0x20`. The magic natives also use `0x80` and `0x10` next to it, consistent with Square (code 2 → bit 7) and Triangle (code 3 → bit 4) (`docs/magic.md` §4).
- **Input struct.** `+0x28` current mask, `+0x2c` previous mask, `+0xe0` left stick, `+0xf0` right stick. CONFIRMED (code). Its address was not resolved in RAM yet.

### 6.7 Weapons and magic: code-confirmed ids

**Player progress block.** CONFIRMED that these are the values the move system reads.
- `0x00335834`: unlock mask.
- `0x0033583c`/`0x00335840`/`0x00335848`: meters (200/200/100 in game).
- `0x00335869`: base level (5).
- `0x0033586b..0x00335872`: per-item levels.
- `0x00335873`: selected magic.
- `0x00335874`: selected item.

**Item id ↔ data name.** CONFIRMED: the same function reads `GBL_Global+off` (whose debug name is the item) **and** calls `FUN_00210690(id)` for that item's level.
- Casting branches require `+0x23 == id` (disc, every `*Enter` branch).
- Weapon movesets are gated by `flagsB` bits for ids 0x0e/0x0f/0x11 (`FUN_0024ea30` + disc, every entry move).

| id | Data name | `GBL` field (stride of stage) | Stages | Cast / use (disc) | Module (code) | Level byte | Level at `ingame1` |
|---|---|---|---|---|---|---|---|
| 1 | Lightning | +0x110 (0x60) | 3 | button 6 pressed, `+0x23 = 1` | 0x001a5830, 0x001a66b0 | 0x33586d | 2 |
| 2 | Electric | +0xf4 (0x5c) | 3 | button 6 pressed, `+0x23 = 2` | 0x0019cb88–0x0019d590 | 0x33586c | 0 |
| 3 | Wind | +0x114 (0xb8) | 3 | button 6 **held**, `+0x23 = 3` | (`FUN_00210728`) | 0x33586e | 0 |
| 6 | Earth | +0xf0 (0x68) | 3 | button 6 pressed, `+0x23 = 6` | 0x0019ac00–0x0019bd90 | 0x33586b | 0 |
| 0x10 | Medusa | +0xe8 | 3 | button 6 **held**, `+0x23 = 0x10` (cast like a magic) | 0x001a7078–0x001a8b20 | 0x335871 | 0 |
| 0x0e | Bone | +0xe0 | 3 | Square/Triangle moveset, flagsB 0x100 (requires 0x0e active) | 0x001942d0–0x00194ab0 | 0x33586f | 0 |
| 0x0f | Hammer | +0xe4 | 3 | moveset, flagsB 0x400 | 0x001a1548, 0x001a19f8 | 0x335870 | 0 |
| 0x11 | Olympus | +0xec | 3 | moveset, flagsB 0x1000 | 0x001aa9d8–0x001ab058 | 0x335872 | 1 |
| — | default (Basic / `ATT_Chains`) | `Weapon` +0xb8 | 6 | moveset with none of the sub-weapon bits | — | 0x335869 | 5 |

**Sub-weapon activation** (`FUN_0024ea30`). An item counts as "active" when `g_0x36a81c & 0x7c00` is non-zero **and** `0x00335874 == id`. CONFIRMED (code).
- In RAM both in-game captures have `& 0x7c00 = 0` (the default weapon in use).
- `g_0x36a81c` is written by functions in the Bone/Hammer/Olympus code ranges (0x00193d70, 0x001a1308, 0x001aa8c8). That these are equip/toggle routines is MEDIUM.

**Magic cost / permission** (`FUN_00210728`, called by the `+0x23` check). CONFIRMED (code).
- For the selected magic it reads that item's stage at the current level (Lightning `+0xc`, Electric `+4` plus a count limit vs `0x002d81f4`, Wind `+0`, Earth `+0`, id 0x10: a fixed 0.01).
- It passes the value to `FUN_00216530`, the meter check. Which meter: MEDIUM (`0x335840` = 200 is the likely magic meter).

**Resources** (`R_PERMA`, all names from the debug index):
- `GBL_Global` (0x70 bytes of global floats);
- `Weapon` (6 stages), `GodMode` (2), `Costume` (17), `Difficulty` (4);
- `NormalHealth/Magic/PowerUp/GodMode` tables;
- `ORB_*` / `ORBE_*` (pickups);
- `IcarusWings`, `HermesShoes`, `Pegasus`.

Their layouts are not decoded.

**Hero-side attachments** (disc):
- `ATT_Chains` = two self-relative pointers to `tChained_101`/`102`, which carry joint names (`goMainBlade`, `LWeapIH`, …).
- `ATT_Olympus` (and the other ATTs) = null + one `tHand`.
- `NB_*Bank` (508 bytes) = `+4` → `tMotionP` plus a table of branch slots (`NB_DefaultBank`: slots 2–15 and 20–23 → `tBranch_129…152`).
- Who selects a bank: not traced. UNKNOWN.

**Retail names.** Not assigned. The data names (`Lightning`, `Electric`, `Wind`, `Earth`, `Medusa`, `Bone`, `Hammer`, `Olympus`) are what the evidence supports. The mapping to GoW II marketing names stays at the levels in the table under §4.

### 6.8 Character fields learned in this pass

| Field | Meaning | Evidence | Confidence |
|---|---|---|---|
| `+0x008` | CRT (`CRT_Hero`) | `FUN_0022c250`, RAM | CONFIRMED |
| `+0x00c` | WAD containing the CRT | `FUN_0018f230` | HIGH |
| `+0x170` | state flags (source of the branch state mask) | `FUN_0024e068` | CONFIRMED (code) |
| `+0x174` | flags; 0x200000 player, 0x400000 tested by branches | ctor, `FUN_0024eb00` | CONFIRMED |
| `+0x178` | **health** (f32): 200.0 at `title`, **193.0** after the user took one hit (`ingame1`/`2`); meter selector 0 | RAM + `FUN_00247bd8` | HIGH (confirm with a second hit) |
| `+0x3c0` | `tSoldier` (CRT field 0) | `FUN_0022c250`, RAM | CONFIRMED |

**Globals.** CONFIRMED (RAM `ingame2`).
- `0x002d8734` → hero controller data (`0x00958660`): the **player pointer**.
- `0x002d870c` → `GBL_Global`.
- `0x002d8924`: a game-side **handle table**, entries `{handle, ?, ptr}` of 12 bytes, index = `handle >> 23`, `handle & 7 == 0`. This refines `hypotheses.md` H-O1, which is about the engine core.

### 6.9 Systems: generic vs player-specific

The `tMemoryPool` objects (type 0xe4, `{u32 hash, u32 count}`) name the runtime **component classes** each controller type allocates. CONFIRMED (disc: hashes resolve through the 0x0f tables).

| Hero (`R_HERO00`, ×1) | Enemy1 (`R_PERMA`, ×30) |
|---|---|
| `hfsmPlayer`, `goSoldier`, `tAnimSystem`, `tFightSystem`, `tMoveSystem`, `tPlayerEffectSystem`, `tFlyingSystem`, `tGrappleSystem`, `tIOSystem`, `tSys`×2, `ConcussionInstanceData`×4, `odbEffect`×10, `odbArrow`×2, `tMove`×2, `SC_Modulate`×2, `SC_VirtualWind` | `hfsmEnemy1`, `goSoldier`, `tAnimSystem`, `tMoveSystem`, `tFightSystem`, `tStandardEffectSystem`, `tMove`×60 |

What follows from this:
- **Generic, enemy-compatible** (HIGH): the Character (`goSoldier`, vtable 0x2f1440, §1), `tAnimSystem`, `tMoveSystem` (the move graph of §6.3–6.5), `tFightSystem`, and the MOV/branch/action data format. Enemy1 uses the same pool types, and the executor `FUN_00247bd8` has ~80 callers.
- **Player-only** (HIGH): `hfsmPlayer` (the HFSM is a code object; its data name only keys a memory pool), `tPlayerEffectSystem`, `tFlyingSystem`, `tGrappleSystem`, `tIOSystem`; the player-flag paths (`+0x174 & 0x200000`, the `0x2d8734` player pointer); the progress block 0x003358xx.
- **Kratos-specific data**: `DC_WAD_R_Hero` (moves, branches, actions, attachments, banks).
- **Weapon / magic-specific**: the movesets gated by flagsB / `+0x23`, the `GBL` item stages, and the item code modules.

**Runtime identification.** Kratos's `tMoveSystem` = `0x00957370` (fields as in §6.4). Its owner context `0x00957230` is probably `tFightSystem` or the HFSM (MEDIUM: the class is not identified).

### 6.10 Not yet known

- `MOV+0x00/+0x04`, `tBranch+0x14/+0x25..`; the semantics of the 25 kind-0x0d action types and of most action case bodies.
- `tCollision` (4-byte objects) and the code that turns them into hit volumes (`FUN_00247968`, `FUN_00232128`, `FUN_0024fef0`, `FUN_00250590`).
- Physical pad bits; the input-struct address.
- `tSoldier`, `tCombat` (floats 100/100/1/…), `tPiggyBank` and `NB_*Bank` consumers.
- The HFSM class and states. ~~Who switches `moveSys+0x6c` when a queued branch fires, i.e. the dequeue side of the ring buffer.~~ (done, §6.4a)
- Damage values: the `tCollision`/`tCombat`/`tActionOverrideDamage` path.

### 6.11 Next targets (recommended order)

1. **Dequeue / transition**: find the reader of `moveSys+0x2b8/+0x2bd`, which writes `moveSys+0x6c`. It proves branch `+0` = target and gives the move-enter path (anim start from `MOV+0x08`).
2. **Runtime checks** (PCSX2, cheap):
   - write watch on `moveSys+0x6c` (`0x009573dc`) while pressing Square → expect `MOV_BasicSquare01`;
   - hold each button and capture, to name the pad bits;
   - take a second hit and watch Character `+0x178`.
3. **`tCollision` → hit volume → damage**: `FUN_00247968` and the other `+0x14` readers; then `tCombat` / `OverrideDamage`.
4. **The kind-0x0d family** (the `0x0024847c` case): Script, PlayFX, SubWeaponToggle, ForceAttachment.
5. **Item modules** (Lightning/Electric/Earth/Wind/Medusa casting; Bone/Hammer/Olympus equip) and `FUN_00216530` (meter check).

## 5. Next targets (first pass; superseded by §6.11)

1. **The `DC_*` handlers** `0x00120588..0x001208b8` and `FUN_001204c0`: find the layout of the tag 0x10 object graph (record header, type ids, the `tBranch` → `MOV` links). This is the key to moves, combos and actions.
2. **`Character_Update` 0x00228f90**: how it reads the move/branch data and the stick input. Runtime: break on its entry with `a0 = 0x00958160` and watch which DC objects it touches.
3. **The HFSM runtime**: what `hash("hfsmPlayer")` is looked up in (EvtServer?), and the states.
4. **Weapon switching**: `NB_*Bank` selection and `ATT_*` activation.
