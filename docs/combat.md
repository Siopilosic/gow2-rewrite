# Combat: hit windows, hit detection, blocking, damage and knock-back

Status: first pass 2026-10-03 (pass7 decompilation, hero move data `R_HERO00`, RAM capture `ingame2`).
Confidence levels follow `docs/confirmed.md`. The move graph (moves, branches, actions) is in
`docs/kratos-data.md` Â§6; this document covers what happens when an attack connects.

## 1. Hit windows (the `tCollision` list of a move). HIGH

Every move (`MOV`, type 0x62) has a packed list at `+0x14` (`docs/kratos-data.md` Â§6.3). Its entries are
**hit windows**: the time span of the move in which one attack volume can hit, and what a hit does.
The disc names them `tCollision_<n>`. `tools/dcmoves.py` writes them to
`analysis/dc/<WAD>/collisions.tsv` (174 in the hero data).

| Offset | Type | Meaning | Evidence |
|---|---|---|---|
| `+0x00`, `+0x02` | half, half | window start / end in normalised move time | `FUN_00247820` |
| `+0x04` | 3 Ã— half | **ground impulse** given to a victim standing on the ground (local x, y, z) | `FUN_0024aa68` |
| `+0x0a` | 3 Ã— half | **air impulse** for an airborne victim | `FUN_0024aa68` |
| `+0x10` | 3 Ã— half | **block impulse** for a victim that blocks | `FUN_0024ae28` |
| `+0x16` | half | **base damage** | `FUN_00249f38` |
| `+0x18` | u8 | attack volume selector (below) | `FUN_00247820` |
| `+0x19` | u8 | flags (below) | `FUN_0024ae28`, `FUN_00247968`, `FUN_00249f38` |
| `+0x1a` | s8 | number of sub-hits spread evenly over the window | `FUN_00247820` |

The halves are IEEE half floats; the code converts them with `(h & 0x8000) << 16 | (h & 0x7fff) << 13`
divided by 2^âˆ’112 (`1.92593e-34`).

**Volume selector `+0x18`.** The caller passes the id of the attack volume that touched the victim.
Values `0x00`â€“`0x1f` must equal it. `0x20`/`0x21`/`0x22` accept the pairs 2â€“3, 4â€“5, 6â€“7; `0x23`â€“`0x25`
are the same pairs for the second query mode (`param_4 != 0`, MEDIUM: projectiles or thrown objects);
`0x26` accepts volume 1 in that mode. In the hero's blade combo the windows alternate between volumes 2
and 3 (`MOV_BasicSquare01` 2, `02` 3, `03` 3 â€¦), and `0x20` is used for moves that swing both: volumes 2
and 3 are the two blades. MEDIUM (from the data pattern).

**Flags `+0x19`.**

| Bit | Reading | Level |
|---|---|---|
| `0x01` | hits only the attacker's lock-on target (`movesys+0x48`) | HIGH |
| `0x02` | spawn the hit effect at the contact (`FUN_0025faa0`) | MEDIUM |
| `0x04` | use the victim's other reference matrix for the effect | LOW |
| `0x08` | not against the player, and no damage branch for it | MEDIUM |
| `0x10` | **cannot be blocked** | HIGH |
| `0x40` | still hits while the victim's moves have flag `0x1000000` (an invulnerable phase) | MEDIUM |
| `0x80` | tests the victim's second block flag (`FUN_00284d08 & 2` instead of `& 1`); in a game mode with `0x36a81c & 0x10000` it is skipped for the player | MEDIUM |

**Sub-hits.** At normalised time `t` inside the window, the hit index is `floor(n Â· (t âˆ’ start) / (end âˆ’
start)) + 1`, at most `n`. Each index can hit each victim once (below), so a window with `n = 10`
(`MOV_AirSpecialSquare`) hits ten times.

## 2. Hit detection

| Function | Reading | Level |
|---|---|---|
| `FUN_00247820(t, window, volume, mode)` | window test: returns the sub-hit index, or 0 when `t` is outside the window or the volume does not match | HIGH |
| `FUN_00247968(move, victim, volume, out_index, mode)` | for the move instance's current time (`clip time / clip duration`), walks the hit windows of its `MOV` (`move+0x6c â†’ +0x14`). Applies the flag filters, then skips any (victim, window, sub-hit) triple already in the move's hit list (`move+0x70`, triples, count byte `+0x57`, at most 48). Returns the window and the sub-hit index | HIGH |
| `FUN_002477c8(move, victim, window, index)` | adds the triple to that list | HIGH |
| `FUN_0024ae28(...)` **Combat_OnContact** | called for a pair of overlapping collision bodies (body flags `+0xf4`; `& 3` = can be hurt, `0x100000`/`0x400000` other pair kinds). It finds the attacker and the victim characters through the body owners' handle lists, asks every active move of the attacker for a hit (`FUN_00247968`), and then resolves block or hit (Â§3) | HIGH (structure), MEDIUM (pair kinds) |

### 2.1 Where contacts come from (read 2026-10-03). HIGH (code), MEDIUM (data layout)

- When a move system is set up (`FUN_002497e8`, owner character at `movesys+0x30`), it registers a
  **contact query** for the owner's collision body with `FUN_00120108`: mask `0x100303`, callback
  `FUN_00249b50`, ignore owner = the character's node handle. `FUN_00120108` copies the query block into
  the node's collision component (component slot `g_SeqIds[0x10]`, block at `+0x20..+0x40`).
- The collision world (`docs/collision.md`) runs these queries and calls `FUN_00249b50` per contact with
  the hit record filled: `0x2fd0c4` collider (owner at `+0x18`), `0x2fd0c8`/`0x2fd0cc` the two touching
  parts, `0x2fd0d0`/`0x2fd0d2` their **material ids**. The callback calls `Combat_OnContact` twice, once in
  each direction, so either side can be the attacker; it returns `0xb` (keep going). A part whose kind
  (`part + DAT_0036b5b8`) is 2 counts as 3 when the body flag `+0xf4 & 0x200` is set.
- `FUN_0023e460` is the same for single queries started elsewhere (it sets `DAT_0036b9d4 = 1`).

**The volume id of a hit window is the attacker part's material id.** The material names come from the
`CDV_` collision definitions (collision server `0x10`, header type `0x00010010`, kind string `BallHull`):

| Material (CDV) | Id | Source | Hit windows using it |
|---|---|---|---|
| `bodyCollisionMat` | 1 | `CDV_gohero` (all `R_HERO*` WADs) | `MOV_RamAttack` (shoulder ram), throws |
| `weaponLeftCollisionMat` / `weaponLeftMat` | 2 | `CDV_gomaiblade` in `R_WEAPON<n>_<stage>` | blade swings |
| (second blade) | 3 | not found in the CDV data; MEDIUM: the second blade instance | blade swings |
| `leftArmCollisionMat`, `rightArmCollisionMat` | 4, 5 | `CDV_gohero` | selector `0x21` (both arms) |
| `leftLegCollisionMat`, `rightLegCollisionMat` | 6, 7 | `CDV_gohero` | selector `0x22` (both legs) |
| `throwCollisionMat` | 9 | `CDV_gohero` | `MOV_AirCombatThrow` (flags `0xb0`) |
| `weaponTwinCollisionMat` | 0x0b | `CDV_gomaiblade` (stages 1â€“5 of some weapons) | not seen in the hero's windows yet |

So selector `0x20` means "either blade", `0x21` "either arm", `0x22` "either leg". HIGH for the ids
(disc), MEDIUM for the blade 2/3 split.

**`CDV_` layout** (MEDIUM, two files compared): `+0x00` type `0x00010010`; `+0x04` `"BallHull"`; `+0x0c`
size; `+0x14` ball count (Kratos 7, blade 1â€“2); `+0x1c` bounding sphere (x, y, z, r); `+0x30` 0x22;
`+0x34` material count; `+0x38â€¦` offsets of the sections (materials at `+0x5c`, 0x40 bytes each: name
32 bytes, `+0x2c` a hash-like word, `+0x30` the id; then per-ball index bytes; then the balls as
`{x, y, z, r}`). Kratos's balls have radii 6, 6, 6.8, 6.8, 12, 13 and 27 units (0.38 to 1.7 m). The
blade's single ball (radius 844.8 units) cannot be a literal sphere at that scale; how blade contacts are
shaped (chain segments?) is open.

## 3. Block or hit (`FUN_0024ae28`, second half)

1. Ignore the hit while the victim's move system or a move has flag `0x1000000`, or while the global
   `DAT_002d86e4` is set, unless the window has flag `0x40`.
2. Ignore it when the surface/material of the attacker (`DAT_0036a820` table) lacks bit 1.
3. Record the hit (`FUN_002477c8`).
4. **Block test:** `FUN_00284d08(victim move system) & 1` (`& 2` for windows with flag `0x80`); if the victim
   is mid-move, `FUN_00249c68` decides first. A window with flag `0x10` is never blocked.
5. **Blocked:** knock the victim back with the block impulse (`+0x10`, through `FUN_0024ad78`), post event
   `0x12` to the victim and `0x0f` to the attacker (`FUN_0024fbd0`), and run `FUN_00247b78` on each
   attacker move. No damage.
6. **Hit:** `FUN_00249f38` applies damage (Â§4). If it returns bit 2, the hit reaction runs
   (`FUN_0024aa68`): an event to the victim (`0x13` default, `0x10`/`0x14` when not bit 4, `0x0d` with bit
   1; `0x11`/`0x0c` when the victim is ridden or attached, bit 8) and the knock-back: the ground impulse
   `+0x04` when the victim is on the ground (`+0x170 & 3`), the air impulse `+0x0a` when it is in the air
   (`+0x170 & 0x104c`). The attacker gets event `0x0e` (hit landed). Event meanings MEDIUM.

**Knock-back** (`FUN_0024ad78`): `FUN_0024ab38` turns the three halves into a world vector (attacker's
frame, MEDIUM), divides it by the victim's mass (Character virtual `+0xbc`), multiplies by 16 and calls
`Character_SetImpulse(victim, v, replace = 1)` (`docs/character-update.md` Â§8.1). It also resets the
victim's gravity scale (`movesys+0x34 = 1`, `+0x38 = 1`, `+0x3c = 0`).

## 4. Damage (`FUN_00249f38`)

`damage = window+0x16 Ã— multiplier(attacker)`, where the multiplier is the attacker controller's virtual
`+0x14` (`FUN_00250710`). The damage is 0 when the victim's moves have flag `0x100` or the victim's
virtual `+0xe4` says it cannot be hurt. A damage filter object at victim `+0x1d0` (virtual `+0x24`) may
change the value. The victim's health is `+0x178` (`docs/kratos-data.md` Â§6.8). Then come the hit effect
(flag `0x02`), the attacker/victim body-pair rules, rumble/sound events (`FUN_00121ac8` with event
`0x414`), and for the player a controller response (`FUN_002135f0`). The damage subtraction is in Â§4.0.

### 4.0 Applying the damage (end of `FUN_00249f38`, read 2026-10-03). HIGH

1. With health `h = victim+0x178 > 0` and `damage â‰¥ h`: the hit is **lethal**; result = 3 and
   `FUN_00247bb8` runs on every attacker move (kill notification).
2. The victim's move system accumulates the damage taken at `movesys+0x50`.
3. Statistics: when the attacker is the player, `FUN_00270828(2, damage)` adds to the counter array at
   `0x0036b0c8` (index 2 = damage dealt; index 3 = damage taken, from `Character_TakeDamage`).
   `FUN_001d0fe0(0x40f, damage, health)` and event `0x3f1` (`FUN_00121ac8` on server `0x13`) report the hit.
4. `Character_TakeDamage` (Character vtable `+0x54`, below). A non-zero return adds bits `4|2` to the
   result (forces the hit reaction).
5. **Super armour:** if the victim is in a move whose instance has a threshold at `+0x64`, the reaction
   bit 2 is cleared while the accumulated damage stays at or below the threshold (a negative threshold
   means always). Windows with flags `0x10`/`0x20` and results with bits `1`/`4` skip this check.

**`FUN_0021beb0(damage, victim)` Character_TakeDamage** (Character vtable `0x2f1440` slot `+0x54`):
- the player takes no damage while `DAT_002d86a4` is set (a cheat or scripted invulnerability, MEDIUM);
- when attached to a host and the CRT flag `+0xe & 0x40` is set, the damage goes straight through;
- otherwise, for `damage â‰¥ 0`, it walks the CRT's **health thresholds** (`CRT+8 â†’ +0x2c` list of
  `{f32 health, s8 id}`). Crossing one calls `FUN_0021bba8(victim, 1, id)` and returns 1. That function
  (string `"goMiniGameCircle"`) starts the **Circle-button finisher minigame** when the player controller
  exists and `0x36a81c & 0x10000` is clear. MEDIUM for the minigame reading;
- new health through `FUN_0021be98`, which clamps it to `[0, +0x17c]`; clears `+0x1a0`.

Related virtuals: `+0xbc` **mass** = `tuning+0x1c` (Kratos 100.0); `+0xe4` **cannot be damaged** when
`+0x318` (Kratos 200.0, the health maximum) is 0. In RAM `ingame2` Kratos has health 193.0 of 200.

### 4.1 Kratos's damage multiplier (`FUN_002143f0`, player controller vtable `0x2f13f8` slot `+0x14`). CONFIRMED (code + RAM)

`multiplier = Difficulty[d] Ã— Costume[c] Ã— weapon factor`, all from `GBL_Global` (`g_GblGlobal`) tables
reached through self-relative pointers:

| Table | `GBL` field | Stride | Index | Values in RAM `ingame2` |
|---|---|---|---|---|
| Difficulty | `+0xc4` | 0x38 (field +0) | `DAT_002d86e0` (1 in the capture) | 2.0, 1.0, 1.0, 0.75 |
| Costume | `+0xc0` | 0x24 (field +0) | `*(DAT_0036a848 + 4)` (1) | 1.0 Ã—11, 0.5, 2.0, 1.0, 1.5, 0.5, 2.0 |
| Weapon (default blades) | `+0xb8` | 0x20 (field +0x10) | level byte `0x335869` (5) | 1.0, 1.5, 2.0, 2.5, 3.0, 1.0 |
| GodMode (when `GameFlags_Test(0x200)`) | `+0xbc` | 0x50 (field +0x30) | byte `0x33586a` | 2.0, 3.0 |
| Bone (when `0x36a81c & 0x400`) | `+0xe0` | 0x38 | `Item_GetLevel(0x0e)` | 1.0, 2.0, 3.0 |
| Hammer (when `0x36a81c & 0x800`) | `+0xe4` | 0x14 | `Item_GetLevel(0x0f)` | 1.0, 1.75, 3.0 |
| Olympus (when `0x36a81c & 0x1000`) | `+0xec` | 0x28 | `Item_GetLevel(0x11)` | 1.0, 2.0, 3.0 |

In the capture the multiplier is 1.0 Ã— 1.0 Ã— 1.0 (blade stage 5 = 1.0), so `MOV_BasicSquare01` deals
2.0 damage. Why the default blades sat at stage 5 with factor 1.0 is open. The difficulty names are not
assigned (index only). The other controller types (enemies) have their own slot `+0x14`.

## 5. Kratos's hit windows (examples, `analysis/dc/R_HERO00/collisions.tsv`)

| Move | Window | Damage | Ground impulse | Air impulse | Block impulse | Volume | Sub-hits |
|---|---|---|---|---|---|---|---|
| `MOV_BasicSquare01` | 0.135â€“0.245 | 2 | 800, 0, 0 | 200, 1000, 0 | 400, 0, 0 | 2 | 1 |
| `MOV_BasicSquare02` | 0.25â€“0.35 | 2 | 800, 0, 0 | 200, 1250, 0 | 400, 0, 0 | 3 | 1 |
| `MOV_AirSquare03` | 0.415â€“0.515 | 4 | 500, 0, 0 | âˆ’50, 1250, 0 | 1000, 0, 0 | 2 | 1 |
| `MOV_AirTriangle01` (2nd) | 0.425â€“0.555 | 2 | 0, 0, 0 | 0, âˆ’3000, 0 | 0, 0, 0 | 3 | 1 |
| `MOV_AirSpecialSquare` | 0.175â€“0.775 | 1 | 100, 0, 0 | 50, 250, 0 | 1000, 0, 0 | both (0x20) | 10 |
| `MOV_RamAttack` | 0.05â€“0.45 | 4 | 1000, 0, 0 | 1000, 0, 0 | 1000, 0, 0 | 1 | 1 (flags 0x06) |

The air impulses lift the victim (juggling) and the air Triangle slams it down, which matches how the
attacks play. The impulse axes (x forward from the attacker, y up) are MEDIUM.


## 6. Hit, kill and block triggers; meter gain on hit. HIGH (code), MEDIUM (meter roles)

The combat code stamps the attacker's move instances, and the action executor (`docs/kratos-data.md`
Â§6.5) fires actions from these stamps:

| Function | Called from | Instance fields | Action trigger |
|---|---|---|---|
| `FUN_00247bb8` | lethal hit (`FUN_00249f38`, Â§4.0) | count `+0x54`, frame `+0x58` | **3 = on kill** |
| `FUN_00247b98` | damaging hit (`FUN_00249f38`) | count `+0x55`, frame `+0x5c` | **1 = on hit** |
| `FUN_00247b78` | blocked hit (`FUN_0024ae28`, Â§3 step 5) | count `+0x56`, frame `+0x60` | **5 = on blocked** |

A trigger fires when `frame + 1 == g_frame` (`DAT_003630d8`), i.e. on the frame after the event. This
upgrades "trigger 1 = on hit" in `docs/kratos-data.md` Â§6.5 from MEDIUM to CONFIRMED (code). Triggers
2/4/6 are probably the count-based variants (open).

**Meters changed by Kratos's moves** (`tActionMeterAdjust`, kind 0x02; amount `+8`, selector `+0xc`;
`analysis/dc/R_HERO00/actions.tsv`):

| Selector | Meter | Max (RAM) | Typical use in the hero data |
|---|---|---|---|
| 0 | own health `+0x178` | 200 | negative amounts on failed minigames and traps (âˆ’5 â€¦ âˆ’35), âˆ’10000 in `MOV_DeathLie` |
| 1 | `0x335840` | 200 | magic-related (`MOV_PriestStruggleMagic` âˆ’5, refills of 200). MEDIUM: the magic meter |
| 2 | `0x335848` | 100 | **gained on hit**: `MOV_BasicSquare01` +0.25 per hit (24 such actions), `MOV_CrushAttack` +0.5, `MOV_AirSpecialSquare` +0.125 per sub-hit, `MOV_SpecialEvade` +1.0; also +1â€¦+10 for minigame steps. MEDIUM: the meter that powers the god-mode state (`GameFlags` 0x200, Â§4.1). The UI calls it `GodMeter` (`docs/hud.md` Â§1, HIGH for the data name) |

Retail names for meters 1 and 2 are not assigned.


## 7. Block, parry and combat events as pseudo-buttons. HIGH (code + data), MEDIUM (event names)

**Block flags.** `FUN_00284d08(moveSys)` returns the OR of the move system's base flags
(`(moveSys+0x44)->+0x10`) and every active move instance's `+0x10`. A hit is blocked when bit 1 is set (bit 2
for windows with flag `0x80`; never for flag `0x10`). The instance bits come from the move's actions
(`MoveSys_ExecuteActions`):
- `tActionBlock` (kind 0x13): `instance+0x10 |= value` (all hero block and parry moves use `0xffffffff`
  over the whole move);
- `tActionSetFlags` (kind 0x01): flag `0x2000000` sets block bit 1, `0x4000000` bit 2, `0x400` sets bit 1 of
  `instance+0x14`;
- kind 0x14 sets `instance+0x14` (a mask that turns off the controller's automatic block below).

`FUN_00249c68(moveSys)`, used when the victim is in no blocking move, asks the controller (virtual
`+0x24`) whether it blocks anyway, unless `instance+0x14` masks it or the character rides a host. For the
**player** controller (vtable `0x2f13f8`) this is `FUN_00284c38`, which returns 0: Kratos blocks only
through his block moves; the automatic block is for enemy AI.

**Events.** After a contact, `FUN_0024fbd0(moveSys, code, other, â€¦)` **MoveSys_PostEvent** evaluates the
branches of the active moves with `code` in the place of a button code (11 callers). The combat codes
therefore drive the move graph exactly like buttons (`analysis/dc/R_HERO00/branches.tsv`, column
`button`):

| Code | Sent to | Hero moves that branch on it | Reading |
|---|---|---|---|
| `0x0c`, `0x11` | victim, when it is ridden/attached | â€” | hit while attached |
| `0x0d` | victim | 113 branches, mostly decision branches (`BRA_*`) | hit (heavy / reaction picked by data) |
| `0x0e` | attacker | `MOV_ThrowReject` (40) and the per-enemy grab moves (`MOV_SoldirKill`, `MOV_HarpyAirThrow`, `MOV_MedusaGrappleX` â€¦) | **hit landed**: a successful `MOV_CombatGrapple` turns into the throw for that enemy class |
| `0x0f` | attacker | â€” in the hero data | attack was blocked |
| `0x10`, `0x13`, `0x14` | victim | `MOV_HurtReaction`, `MOV_HitBounce`, `MOV_TrHitDrop`, decision branches | hit reactions (light / other / flag `0x80` windows) |
| `0x12` | victim | `MOV_BlockReaction`, `MOV_BlockBreak`, `MOV_ParryPush`, `MOV_GoldenAbsorb(P)` | **blocked a hit** |

**Kratos's block and parry** (hero data):
- `MOV_Block` is entered by holding button code 5 (the block button) and stays while it is held. On `0x12`
  it goes to `MOV_BlockReaction` or `MOV_BlockBreak` (the branch conditions decide, e.g. the attack's
  window flags or the health ranges at branch `+0x16..+0x1d`). From `MOV_Block`, the face buttons go to
  `MOV_Special*`, the right stick (code `0xb`) to the evades, and the magic button (code 6) to the casts.
- **Parry:** holding block during an attack (`MOV_BasicSquare01..05` â€¦) enters `MOV_Parry`, or
  `MOV_GoldenParry` with unlock requirement 5. `MOV_Parry` blocks everything and, when it blocks a hit
  (`0x12`) in the first 60 % of the move (window 0â€“0.6), branches **immediately** (flag `0x400`) to
  `MOV_ParryPush`. That move slows time (`tActionSlowdown`), pauses the hit counter, plays the parry
  sounds and the enemy sound (`tActionSoundOnEnemy` on trigger 1). After 0.6 the parry becomes a plain
  `MOV_Block` if the button is still held.
- Each weapon moveset (Bone, Hammer, Olympus) has its own block / reaction / break / parry / golden parry.

So the parry is a timing window in the data, not special code: the same block flag as `MOV_Block`, plus a
branch on the "blocked" event inside the window.


## 8. Hit feedback: hit-stop, slow motion, air float, camera shake, rumble. HIGH (code), values CONFIRMED (disc)

These are action kinds of the move graph (executor `MoveSys_ExecuteActions` `0x00247bd8`; kind numbers as
in `docs/kratos-data.md` Â§6.5). Values from `analysis/dc/R_HERO00/actions.tsv` (field `raw`, halves at
`+8`/`+10`).

| Kind | Type | What the executor does | Hero values |
|---|---|---|---|
| 0x15 | `tActionHitPause` | stores the action in the move instance (`+0x08`). When that move deals damage (`FUN_00249f38`), the duration `half(+8)` Ã— 1000 ms goes out as event `0x414` to **both** the attacker and the victim bodies (server `0x13`): the **hit-stop** freeze | 0.05 s (44 moves, e.g. `MOV_RamAttack`), 0.075 s (12, `MOV_AirSquare03`), 0.025 s (11), 0.1 s (10) |
| 0x0c | `tActionSlowdown` | global slow motion: `DAT_0036b9dc = half(+8)` (time scale), `DAT_0036b9d8 = half(+10)` (duration, s) | 0.25 for 1 s (grab sequences), 0.05 for 0.25 s (`MOV_GoldenAbsorb`), 0.15 for 0.5 s (`MOV_CombatJump`) |
| 0x0f | `tActionVelocityScale` | sets the move system's gravity-scale target `+0x38 = half(+8)` and its timer `+0x3c = half(+10)`; `MoveSys_Update` eases the live scale `+0x34` toward it by 10 % per frame and restores 1.0 when the timer runs out. Gravity is multiplied by `+0x34` (`docs/character-update.md` Â§6.2) | **air float**: `MOV_AirSquare01` 0.25 for 0.5 s (19 moves) or 0.05 for 0.5 s; `MOV_AirTriangle01` 0.15; `MOV_CombatFall` 0 / 0 |
| 0x0a | `tActionCameraShake` | `FUN_00234f60(camera DAT_002d8dcc, preset)`; the preset is an import resolved to a `CSH_*` object in `R_PERMA` | 290 actions |
| 0x0b | `tActionForceFeedback` | `FUN_00267930(preset)`; preset = `FFB_*` | 315 actions |
| 0x11 | `tActionTimeWarp` | `FUN_00150388(preset)` | 9 |
| 0x0e | `tActionOverrideDamage` | instance `+0x64 = half(+8)`: the **super-armour threshold** read in Â§4.0 (so the debug name is about damage the move can absorb) | 11 |
| 0x12 | `tActionContext` | instance `+0x0c = s8(+8)`: the context id passed with the combat events (Â§7) | 101 |
| 0x17 | `tActionHitCounterPause` | for the player: `g_0x36a81c |= 2` | 242 |
| 0x18 | `tActionDeathMenuPause` | for the player: `g_0x36a81c |= 4` | 17 |
| 0x1b | `tActionTimer` | compares `half(+8)` with the time in moves (`moveSys+0x54`) and sets instance flag `+0x2b4` bit 0 or 1 | 8 |

`MOV_ParryPush` (Â§7) uses Slowdown, HitCounterPause and SoundOnEnemy together: the parry's slow-motion
moment is data.


## 9. Death. HIGH (code + data)

1. Damage clamps health at 0 (`Character_SetHealth`, Â§4.0).
2. Every frame the move system evaluates its moveset bank (`FUN_0024f6c0`, called from `MoveSys_Update`
   when no move is pending; ties between equal-score bank branches are broken at random with
   `FUN_0014be00`). When **health â‰¤ 0** and no code-requested move is queued, it posts event **`0x0d`** to
   the character itself (`MoveSys_PostEvent`, Â§7).
3. The bank branches that react to `0x0d` choose the **death move by locomotion state**
   (`analysis/dc/R_HERO00`, `BRA_*` objects; state mask in flagsA, `docs/kratos-data.md` Â§6.4):

   | Branch | Move | State mask |
   |---|---|---|
   | `BRA_DeathStumble`, `BRA_DeathFlyBack` | `MOV_DeathStumble`, `MOV_DeathFlyBack` | ground (0x1) |
   | `BRA_DeathAir` | `MOV_DeathAir` | air (0x2) |
   | `BRA_DeathRope` | `MOV_DeathRope` | rope (0x4) |
   | `BRA_DeathWall` | `MOV_DeathWall` | wall (0x8) |
   | `BRA_DeathWater` | `MOV_DeathWater` | water (0x10) |
   | `BRA_PegasusDeath` | `MOV_PegasusDeath` | Pegasus (0x40) |
   | `BRA_DeathCeiling` | `MOV_DeathCeiling` | ceiling (0x80) |
   | `BRA_DeathPlane`, `BRA_DeathSmash` | `MOV_DeathPlane`, `MOV_DeathSmash` | any state (0x1bf); other conditions decide |

   Many scripted-sequence moves (minigame fails, `MOV_StoneStatue`, boss finales) have their own `0x0d`
   branch with flagsA `0x10000` ("do nothing"), which **suppresses death** during those sequences.
4. The death moves adjust the meters (`tActionMeterAdjust`; `MOV_DeathLie` sets health âˆ’10000).
   `tActionDeathMenuPause` (`g_0x36a81c |= 4` for the player) appears in the instant-death and
   minigame-fail moves (`MOV_IcarusCrashDeath`, `MOV_CerbusMaleFail` â€¦), not in the standard death moves;
   how a standard death reaches the continue menu (`DeadMenu_Event`, `docs/hud.md`) is open. A fall that lasts 8 s or a forced kill (`DAT_002d8920`) goes through
   `FUN_0024bc40` with damage `max health Ã— 100` (`docs/character-update.md` Â§3.2).


## 10. Weapon switching (default blades â†” sub-weapon). HIGH (code), MEDIUM (button)

State: `g_0x36a81c` bits `0x400` Bone, `0x800` Hammer, `0x1000` Olympus active (`0x7c00` = any
sub-weapon, `docs/kratos-data.md` Â§6.7); bit `0x8000` = **toggle requested**; selected item `0x00335874`.

1. **Request** (`FUN_00213c60`, player controller, every frame): when sub-weapons are allowed
   (`0x00335875` non-zero, not `DAT_002d86e4`) and one is unlocked (`g_UnlockMask & 0x2000/0x4000/0x10000`),
   a new press of pad bit `0x2` (input `+0x28` against `+0x2c`) sets `0x8000`. The physical button is not
   identified (MEDIUM; a runtime test will settle it).
2. **Forced off:** on a wall (unless ladder/slide sub-modes), on a ceiling, in water, on a grapple, on Pegasus
   or hand-over-hand on a rope, the sub-weapon is put away (`FUN_00213a98(player, 1)`) and the request
   bits are cleared.
3. **Toggle at a safe point:** every locomotion move (`MOV_Stand`, `MOV_Walk`, `MOV_Jump` â€¦) carries a
   `tActionSubWeaponToggle` action (140 in the hero data), which runs `Scr_SubWeaponToggle`
   (`0x001bbd40`). If the request bit is set it equips (`FUN_00213968`) when no sub-weapon is active, or
   unequips (`FUN_00213a98`) otherwise, clears the request and re-evaluates the move (`FUN_0024f0f8`).
4. **Equip** (`FUN_00213968`): only when the weapon WAD for the selected item and its level is loaded
   (`DAT_0036b18c+0x18 == level | id << 16`, flag `+0x1c & 8`; the `R_WEAPON<id>_<stage>` WADs). It calls
   the item's equip function: Bone `FUN_00193d70`, Hammer `FUN_001a1308`, Olympus `FUN_001aa8c8`. Each sets
   its active bit and **pushes its moveset bank** on the animation controller's stack
   (`FUN_00213fa8(player, "NB_BoneBank")`, â€¦), then refreshes (`FUN_00213ed0`). Unequip clears the bit and
   pops the bank (`FUN_00214080`). The animation state restarts (`ctrl+0xdc = ctrl+0xd8`).
5. With a pushed bank the moveset stack lookup (`docs/character-update.md` Â§6.4) and the moves' flagsB
   gates (`docs/kratos-data.md` Â§6.7) select the sub-weapon's moves; the damage multiplier uses that
   weapon's level table (Â§4.1).

Scripts can also force it: `SCR_EnableSubWeapon`, `SCR_DisableSubWeapon`, `SCR_HideSubWeapon` (the last
is used at the start of every cast move, `docs/magic.md` Â§1).

## 11. Entry branches, the stance and the move loop (read 2026-10-04 while porting). HIGH (code + data), MEDIUM where marked

Ported in `gow2-kratos/src/moves.rs`; the data reader is `gow2-formats/src/dc.rs` (matches `tools/dcparse.py`/`dcmoves.py` on all 1,041 hero moves, `tests/dc_oracle.rs`).

**Where a move starts from nothing.** `CRT_Hero + 0x18` is a packed list of 250 entry branches (`BRA_SquareAttack`, `BRA_TriangleAttack`,
`BRA_ParryTap`/`Hold`, `BRA_EvadeF/B/L/R`, the casts, the air variants, the Titan, Bone, Hammer and Olympus sets, the hit reactions). The
array sits at the object that the debug index calls `TweenOut` (it names the nearest object, not the array). `MoveSys_Update` evaluates
it through `FUN_0024f6c0` when the character is on the ground, no host and no `0x800000` flag, and **every active move has `MOV+4 & 8`**
(locomotion moves have it, attacks and stances do not). Each branch is tested with `FUN_0024e210` (input) and `FUN_0024eb00` (state); the
branch with the lowest specificity value wins (ties at random). `BRA_SquareAttack` is button 2 just pressed, ground, to
`MOV_BasicSquare01`; `BRA_AirSquareAttack` is the same with state 2.

**The input test (`FUN_0024e210`).** Button codes 1 to 10 map to pad bits through `0x002f6da7` (`dc::BUTTON_BIT`), press modes 1/2/3-4/5 are
pressed, released, held, not held. Code `0x0b` is **unconditional** (no button): with stick condition 0 it is always true, with a stick
condition it is a stick-direction branch (the evades). Codes `0, 0x0d, 0x10, 0x12-0x16, 0x1c-0x20` never match input (they are event
codes, section 7). `0x17` = an attack button was just pressed, `0x18` = the stick is used (a move can be walked out of after 40 % of its
clip: every `MOV_BasicSquare*` has a null-target `0x8000` branch on it). Both are MEDIUM (`FUN_0024e1d8/e120/e180` are not read).
Stick conditions `+0x22`: 1/2 magnitude below/above 0.8; 7, 8, 9, 10 = left, right, forward, back with the stick at least 0.8 out
(the names come from `BRA_EvadeL` 0x11, `R` 0x12, `F` 0x13, `B` 0x14); `0x0b`-`0x14` are the same tests on the right stick.

**The combat stance.** `MOV_BasicSquare*` end through an unconditional `0x0b` branch (window 0 to 1, queued only once the window has run
out, so it fires at the end of the clip) into `MOV_CombatIdle`, which loops on its own `0x0b` branch. Its only exit in the data is the
`0x17` branch with flagsA `0x8000` ("only end the move"): an attack button ends the stance, and in the same update the entry list is
evaluated with the same just-pressed edge, so the Square press starts `MOV_BasicSquare01`. How the stance returns to standing and
walking is **not in the move data** (LOW: `MoveSys::leave_stance_when` ends it on walking and after 4 s).

**Branch selection (`FUN_0024f250`, `Branch_Score`).** Window, input, state; score 1 for `0x0b`, 3 for others, 4 immediate (`flagsA & 0x400`),
5 priority (`0x800`); held mode 3 is excluded unless already buffered. The winner is queued with time = window end, except immediate
branches (time 0); a branch with button `0x0b` or press mode 3 is not queued before its window has run out. Due when the move's
normalised time reaches the queued time; at the end of the clip with nothing queued the move ends. Confirmed by the port: Square taps in
`[0, 0.2749]` wait for 0.2749 (buffered), taps in `[0.2749, 1]` act at once, and five taps give `BasicSquare01` to `05` and the finisher.

**State test (`FUN_0024eb00`).** flagsA low bits against the state mask (1 ground, 2 air); unlock requirement (`+0x23`, inverted by
`0x2000`); `0x4000` target present; `0x200`; the level (`min_level` against the base level, 5 in the captures); flagsB:
`0x10/0x20` god mode, `0x40/0x80`, `0x1/0x2` target object, `0x4/0x8`, `0x10000-0x20000`, `0x4000-0x8000` and the sub-weapon gating
`0x100-0x2000`. The gating (`FUN_0024ea30`, read from the decompilation, HIGH, 2026-10-04; ~~the earlier reading from the data pattern, "lower bit allows
while active, upper while not", was wrong~~): the function gets the selected magic id (`DAT_00335874`, one of `0xe`, `0xf`, `0x11`) and whether a
sub-weapon is active. Each of the three ids has a bit pair: the upper bit (`0x200`, `0x800`, `0x2000`) means the branch does not care, the lower
(`0x100`, `0x400`, `0x1000`) requires that id to be the active one, and with neither the branch is refused while that id is active. A branch with none
of the six bits is a default-blades move; `0x2aaaa` works with anything. The wrong reading refused the evades (`0x2a2aa`: no bit for the middle id),
so Evade F/B/L/R never started; they do now (right stick, entry branches `BRA_Evade*`, stick codes `0x11` to `0x14`). Then the window, the health ranges and
the target class compares. The returned specificity bits: 2 when there is no unlock requirement, 4 and 8 for target wildcards.

**Actions.** `tActionHitPause` (0x15) has trigger 0: it **stores** the pause (0.05 s for `BasicSquare01`) when the move starts, and the
combat code uses it when damage is dealt (event `0x414` to both bodies). Meter and shake actions of attacks use trigger 1 (on hit) and fire
on the frame after the hit. Both are what the port does (`tests/combo.rs`: one hit, 2 damage, 128 units/s knock-back, the god meter +0.25
and the pause appear).

**Root motion is in `zeroJoint` (2026-10-04, HIGH on the data, MEDIUM that the engine uses exactly its delta).** ~~"No root motion in attack clips"~~: joint 0 and the
pelvis do stay put, but the clips move the origin joint `zeroJoint` (joint 120, the one the concussions use as an origin; `synchJoint` 121 and `linkJoint` 122 stay at
their offsets). In rig space x is right and -z forward: `defEvadeF` moves it 146.5 units forward (about 9 m in 1.2 s), `defEvadeR` 146 units to the right, `attBrutalSlash`
61 units forward; `navIdle` and the walk cycles do not move it. It is not a skinned joint, so the mesh stays in place and the game must move the body by its change.
The port does that (`kratos_play.rs root_motion`: the delta of `zeroJoint` between two ticks, turned by the heading, handed to `Body::tick_world` as `Controls::drive` so walls
and floors still apply): attacks step forward and the evades roll. Probe: `gow2-kratos` example `root_motion`.

**Being hit, blocking, evading, dying in the port (2026-10-04).** The dummies of `kratos-play` can fight back (`gow2-kratos/src/enemy.rs`, key V): a fixed melee loop
(walk up at 3.5 m/s, 0.55 s wind-up, strike within 34 units, 0.7 s recovery; every third blow heavy). It is a test aid; the game's enemy behaviour (`BHV_*`) is not decoded (LOW).
What happens to Kratos uses the game's own moves: a blow while a move named `*Evade*` runs misses; while `MOV_Block` runs it costs 10 % and is registered as blocked (`register_blocked`);
otherwise it costs 8 (light) or 18 (heavy) health, pushes him, and posts event `0x10` with hit class 10 (light, starts `MOV_HitFront`) or 11 (heavy, `MOV_HitBlowBack`) so
the entry branches `BRA_Hit*` pick the reaction (their `b1f` is the class; the other hit classes, 1 to 9, 19 to 26, select spin, kneel, floored, fly back, bounce, ceiling, wall
and water reactions). At zero health event `0x0d` (the death branches tie at random: stumble, fly back, plane, smash ...) is replaced by `MOV_DeathFlyBack` after a heavy blow and
`MOV_DeathStumble` after a light one; after 3.5 s he gets up at the start with full health. Pressing block at once gives `MOV_Parry` first, then `MOV_Block`.

## 12. Collision balls: joints, ids and the blade contact shape (decoded 2026-10-04). HIGH (data + RAM geometry), MEDIUM where marked

Reader: `gow2-formats/src/cdv.rs` (`tests/cdv_balls.rs`). This closes the two open items of section 2.1.

`CDV_*` layout beyond section 2.1: the section offsets at `+0x38` are `[0x40, materials, joints, material indices, balls, end]`. After the
0x40-byte materials come **one byte per ball with the joint it hangs on**, then **one byte per ball with its material index**, then the
balls (`f32 x, y, z, r`, **in the frame of that joint**). The world centre is `centre x joint world matrix`.

| Ball | Joint | Centre | Radius | Material (volume id) |
|---|---|---|---|---|
| 0 | 34 `lWrist` | (-1.6, 2.3, 1.2) | 6.0 | `leftArmCollisionMat` (4) |
| 1 | 51 `rWrist` | (1.3, 2.4, 1.4) | 6.0 | `rightArmCollisionMat` (5) |
| 2 | 110 `lMetatarsal` | (0.1, 1.8, -0.1) | 6.8 | `leftLegCollisionMat` (6) |
| 3 | 114 `rMetatarsal` | (-0.1, 1.8, -0.1) | 6.8 | `rightLegCollisionMat` (7) |
| 4 | 2 `pelvis` | (0, 7.2, -1.2) | 12.0 | `bodyCollisionMat` (1) |
| 5 | 2 `pelvis` | (0, -10.9, -2.0) | 13.0 | `bodyCollisionMat` (1) |
| 6 | 2 `pelvis` | (0, -3.0, -20.0) | 27.0 | `throwCollisionMat` (9) |

So the "arm" volumes are the fists and the "leg" volumes the feet. The blade's `CDV_gomaiblade` has two balls with the same shape
(centre (1.665, 0, -319.0), radius 844.8) and materials `weaponLeftMat` (2) and `weaponTwinCollisionMat` (0x0b). The blade's rig joint
carries the scale 1/64, so the ball is a sphere of radius **13.2 units** with its centre 5 units down the blade, in the middle of the
mesh (which spans z = -16.8 to 1.3 in those units): the earlier doubt that 844.8 "cannot be a literal sphere" came from ignoring the joint's scale.
The right blade gets volume id 3 (MEDIUM: the pair 2-3 that the `0x20` selector accepts).

Port: a hit window's selector matches balls by id (`combat::volume_matches`: exact below `0x20`, `0x20` blades, `0x21` fists, `0x22` feet),
each ball is carried by its joint (hero) or by the blade's matrix, and a hit needs a matching ball to touch the victim; the path the ball
travelled since the last tick is tested too (blades move far per tick). `F3` in `kratos-play` draws the balls.

## Open questions

- ~~Attack volumes in the port are a reach in front of the attacker (LOW). Which joints carry the CDV balls and how blades make contact is open.~~ Done, section 12.
- How the stance returns to walking; where the forward step of attacks comes from; `FUN_0024e1d8`, `FUN_0024e120`, `FUN_0024e180`.
- ~~The collision bodies and attack volumes~~ (ids and source found, Â§2.1). Open: which joints carry the
  balls (the per-ball index bytes), the blade contact shape, the id 3 / id 0x0b blades.
- ~~The rest of `FUN_00249f38`: health subtraction, death~~ (read, Â§4.0); ~~where health 0 turns into the
  death state~~ (Â§9); rage/magic gain on hit (`tActionMeterAdjust` trigger 1 is "on hit", `docs/kratos-data.md` Â§6.5).
- `FUN_0021bba8` in full (the finisher minigame start).
- `FUN_0024ab38` (impulse frame). ~~The victim mass (virtual `+0xbc`), event codes `0x0c`â€“`0x14`~~ (Â§4.0, Â§7).
- ~~The block flags in `FUN_00284d08` and the parry path~~ (Â§7). Open: what decides `MOV_BlockReaction` vs `MOV_BlockBreak`.
- Damage to Kratos: the enemy controller's slot `+0x14`.


