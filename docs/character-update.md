# Character_Update (0x00228f90): structure and callees

_Started 2026-10-03 (ledger kr-04). Working document: sections are filled in as the 11,012-byte
function is read. Field names follow the offsets; `this` = Character (vtable 0x2f1440, slot 2).
Runtime anchor: Kratos `this = 0x00958160`, called from EvtServer_Update (runtime-validation.md §16)._

## 1. Entry and modes. HIGH (code)

`Character_Update(this, msg, arg)` takes a message argument (`param_2`).

| Condition | Action |
|---|---|
| always first | for each of the `*0x2d8e32` grids (`0x2d8e38[i]`): `FUN_0021b7d8(this, grid)`; then `FUN_0021ba98(this)`; then the capsule refresh (`FUN_002233d0`, or `FUN_00223478` when `+0x174 & 0x200000`) |
| `+0x170` bit 13 (`0x2000`): **attached to a host** | find the host through the handle at `+0x1bc` (handle table 0x2d8924); copy the host joint matrix (joint `host+0xf2`) into own joint `+0xf2` space; scale by `+0x168`; optional ground probe `FUN_0021d150(this, pos, 0x28000000, 0)` writes the move system's displacement (`+0x1c8 → +0x10`); set matrix (`FUN_0021b058` immediate + `Node_SetMatrix`); then `FUN_0024cb78(movesys)`; return |
| `msg == 2` | `FUN_0022bd20(this)` and return its result (MEDIUM: a secondary/post update; it calls `FUN_00221d80` and `FUN_00221a88`) |
| `msg == 4` | **reset**: release a held target (`FUN_001f21e8` if `+0x170 & 0x400`), `FUN_0021c398`, `FUN_00225780(dt, this, 2)` unless `+0x170 & 1`; if the handle at `+0x3e0` resolves, snap the node and both own matrices to that object's world matrix; then `FUN_00225318`, `FUN_0022bab0`, and `(+0x3c4)->+0xd8 = -1`; return 0 |
| otherwise | the main per-frame update (section 3) |

## 1.1 Locomotion state word `+0x170`. HIGH (code `FUN_0024e068` + branch data)

`FUN_0024e068` converts `+0x170` (and `+0x174`) into the branch state mask (`tBranch` flagsA low 9 bits). Grouping the R_HERO00 branches whose mask has exactly one bit names each state from the moves that use it:

| `+0x170` bits | Mask bit | Moves restricted to that mask (examples) | State |
|---|---|---|---|
| `0x3` | 0x1 | BasicSquare01–05, BasicTriangle, Evade F/B/L/R, CombatGrapple (466 branches) | **ground** (bit 0 / bit 1, split not yet known) |
| `0x104c` (bits 2, 3, 6, 12), unless `+0x174 & 1` or `0x1000` | 0x2 | AirSquare01–03, AirSpecial*, CombatJump, CombatFall (90) | **air** (jump / fall / double jump) |
| `0x400` with `+0x174 & 0x20` | 0x4 | RopeKick, RopeSlash, RopeThrow, RopeJump, HitRope | **rope** |
| `0x20` (unless `+0x174 & 0x700`) | 0x8 | WallSlideEnter/Loop, WallSlash, WallThrow, HitWall* | **wall climb** |
| `0x10` | (none alone) | WallHang, WallHangL/R (handler §10) | **ledge hang** (2026-10-03, MEDIUM) |
| `0x380` (bits 7–9) | 0x10 | HitWater, DeathWater, DiveDashCollide | **water** (three sub-states) |
| `0x1000` | 0x20 | only in the "any state" mask 0x1bf (Climb A*, deaths, intro) | UNKNOWN (open; not Icarus, whose moves use 0x3) |
| `0x4000` (bit 14) | 0x40 | LightAttackL/R01–03, Pegasus* (36) | **Pegasus flight** |
| `0x800` | 0x80 | CeilingSlash01–03, CeilingThrow, CeilingSlam, HitCeiling* | **ceiling hang** |
| `0x8000` | 0x100 | GrappleHangEnter, GrappleHangIdle | **grapple hang** |

## 2. Recurring pattern: move-system flag query (inlined many times). HIGH

`ms = this+0x1c8` (move system).
1. Unless the move system's owner is attached (`FUN_00284d00(ms)+0x170 & 0x2000`), walk the active list `ms+0xa8`. For each entry whose `+0x50` differs from the global frame counter `DAT_003630d8`, set it and run `MoveSys_ExecuteActions` (`FUN_00247bd8`). Actions therefore execute **at most once per frame**, lazily on first query.
2. Then `mask & (FUN_00284cf8(ms)+0x5c | Σ entry+0x2b0)`: the current move data's flag word, ORed with each active entry's flags.

Masks seen in the main update: `0x600000`, `0x200000`, `0x800`, `0x8000000`.

## 3. Main update (msg not 2/4). Partial reading, lines in order

1. If `+0x174 & 0x8000`: clear the velocity pair `+0x380/+0x3a0` and flags `+0x3b8 &= 0xfffe1200`.
2. `FUN_00225318(this)`, then `FUN_00227c18(dt, this)`.
3. Movement-mode byte `m` = `(ms+0xb0)->+0x6c` byte 5, or 0x1f with no current move. Bits 0x18, 0x1c, 0x08 and 0x02 gate the locomotion paths below (meaning MEDIUM: which motions the current move allows).
4. If no active entry has `(+0x6c)->+4 & 8`: `+0x340 = 0` and `FUN_001fae80(this)`.
5. Root motion: when the move flags (`0x600000` …) say so, the node's displacement comes from the animation; otherwise from the computed velocity. The result goes to `ms+0x00`/`+0x10`.
6. If `+0x170 bit 14` is clear, the **ground/locomotion branch**:
   - `+0x174 & 4` (holding/grabbing) selects a grab path. Moves 0x5d / 0x5e ending (`(+0x3c4)->+0xd8`, progress ≥ 1.0) release the held object (`FUN_0021c398`) and go to `FUN_00225aa8` / `FUN_001f6ed8`.
   - Otherwise by `+0x170`: `0x800` → `FUN_002261a0`, `0x20` → `FUN_001f3d00`, `0x10` → `FUN_001f3ff0`, else `FUN_00225aa8`.
   - Then `FUN_00221d80` (velocity), `FUN_00226af0(dt, this, v, …)` (movement/collision, returns the new position), `Node_SetMatrix`, `FUN_002221b0`, `FUN_00223fa8`.
7. ~~If `+0x170 bit 14` is set (airborne or special):~~ Corrected the same day: bit 14 is **Pegasus flight** (§1.1), and the branch at step 6 tests bit 14 the other way round, so steps 6 and 7 must be re-read with that in mind. The code path of this step: timers count down by dt at `+0x340`, `+0x334`, `+0x330` (clears `+0x174 bit 0`), `+0x33c` and `+0x348`. Combat-target flags in `+0x37c` (0x200/0x400/0x800/0x1000) → `FUN_001f2df8(this, mask)`. With `+0x37c & 0x8000` and game mode `DAT_002d8ec0 == 0xb` → `FUN_001f1110`. Then by `+0x174`: bit 2 → `FUN_001f1388`, bit 3 → `FUN_001f1ff8` …

_First full pass done 2026-10-03 (§3.1–3.3). Still to read in detail: the handlers in §3.1, `FUN_00226af0` (movement/collision), `FUN_0023ad28` (animation), `FUN_00221d80` (velocity), and where the string "PegCamera01" is used._

## 3.1 Locomotion handlers (dispatch on §1.1 states). MEDIUM (dispatch HIGH from code; handler meaning from the state)

| Handler | Size | Chosen when | Reading |
|---|---|---|---|
| `FUN_002284c8` | 2740 | no special state | ground locomotion |
| `FUN_001f6000` | 1976 | `+0x170 & 0x30` and movement byte `& 0x18` | wall-climb movement |
| `FUN_001fb298` | 1412 | `+0x170 & 0x1000` | state 0x1000 (unknown) |
| `FUN_001f7cd8` | 2644 | `+0x170 & 0x380` | water |
| `FUN_001f8db0` | 3480 | `+0x170 & 0x8000` | grapple hang; posts event `0x422` with `(+0x3d0)->+4` when it ends |
| `FUN_001f3168` | 2964 | `+0x170 & 0x400` | rope (and other 0x400 traversal) |
| `FUN_00227f28` | 1436 | movement byte `& 0x18 == 8`, `+0x170 & 1`, no move flag `0x8000000`, `+0x174 & 0x80` clear | ground variant (MEDIUM) |
| `FUN_001f1388` / `FUN_001f1ff8` | 3180 / 240 | `+0x174 & 4` / `& 8` | held / carried variants |
| `FUN_002261a0` / `FUN_001f3d00` / `FUN_001f3ff0` / `FUN_00225aa8` | | ground branch by `0x800` / `0x20` / `0x10` / else | state entry/exit helpers (MEDIUM) |

After the handler the node is moved with `Node_SetMatrix`. Special finishers follow: `FUN_001fa1b8` (`0x8000`), `FUN_001f24d0` (`0x400`), `FUN_001f7350` (water). Then the **animation blend inputs** are computed: `afStack_230` = horizontal speed / 16 (`|v| · 0.0625`), plus a normalised direction pair, clamped to [−1, 1] (`fStack_238` / `fStack_234`). `FUN_001f67d0` computes them for the wall and climb states.

## 3.2 Pegasus mount path (`+0x170` bit 14). HIGH (code), MEDIUM (object identity)

When bit 14 is set, the update asks `FUN_001b19b0()` (12 bytes, returns a global object). If it returns 0, the ordinary locomotion path (`LAB_0022a170`) runs instead.
- First frame (`+0x174 & 0x10000000`): clears the flag, resets the move system (`FUN_00249a48`), sets the anim controller (`+0x3c4`) to animation `0x6f`, and calls `FUN_0023ad28` / `FUN_0024cb78` / `FUN_00101808` / `FUN_001b4e28` (mount setup).
- `Node_SetMatrix` with the character's own matrix, then position `+0xe0` = mount `+0x120`.
- Unless the move flags `0x600000` (own, or the mount's move system `(*mount)+0x1c8`) are set, the rider joint `+0xf4` of the mount's skeleton gets an orientation built by look-at (normalise, cross product, re-orthogonalise).

Bit 14 = Pegasus flight (§1.1), so `FUN_001b19b0` is most likely the **Pegasus / mount object getter**. MEDIUM. The pad bits documented as "bits 8/4 of `FUN_001b19b0()+0x6ec`" (kratos-data.md) then belong to the mount object, which should be rechecked.

## 3.3 Common tail. HIGH (code), meanings MEDIUM

1. `FUN_0022bab0(this)`; `FUN_00225720(this)` when `+0x174 & 0x2000000`.
2. **Animation update:** `FUN_0023ad28(blendA, blendB, blendC, x, 0, 0, speed, scale +0x168, anim controller +0x3c4, rootMotion)`.
   - `rootMotion` = 1 when there is no current move or the move's `(+0x6c)->+4 & 8` is set.
   - A non-zero result is stored in `+0x340`.
3. **Turning:** unless `DAT_002d86e4`: `vtable+0x54(this, −(+0x188 · dt))` (a yaw rate).
4. **Death and kill checks:**
   - A forced kill (`DAT_002d8920`) or `+0x170 & 8` (air) with `+0x34c ≥ 8.0` (MEDIUM: fall time in seconds) runs `FUN_0024bc40(+0x17c · 100, movesys, …, 0xe)` and sets `+0x174 |= 0x10000`. That is a lethal hit.
   - If `+0x374 & 0x10000` is clear and **`+0x178 ≤ 0`**, death handling follows: message `0x3f0` to `DAT_00362c94` via `FUN_00121ac8` (water states also set `+0x174 |= 0x8000`), or otherwise reset the velocity, `FUN_00225780(dt, this, 2)`, release target and held object, and set `+0x174 |= 0x8000`.
   - This supports **`+0x178` = health** (ledger rt-07, still to confirm at runtime). HIGH (code).
5. If `+0x3dc` is set and `+0x37c & 0x400000`: `(+0x3dc)->vtable+0x24(obj, this)`.
6. If `(+0x1b4)->+4 & 4`: `FUN_001ba3b0(node, pos + up · radius/2 · 16, +0x3d8, this)` (MEDIUM: the shadow/blob placement; same call as in the attached path).

## 4. Callees identified so far

| Function | Size | Reading | Level |
|---|---|---|---|
| `FUN_0013c408` | 132 | **Node_GetDeltaTime(node, channel)**: frame dt `DAT_002d80dc` × time-scale table `0x2d80c8[channel]` × the product of node speed factors `+0x110` up the parent chain (while `+0xf8 & 0x10`). Used as `dt` everywhere | HIGH |
| `FUN_0021b058` | 88 | **Character_SetMatrixImmediate**: writes the 4×4 to `+0x50..+0x8c` and the same into the previous-frame copies `+0x90..+0xdc`; resets scale `+0x150..+0x15c` to 1. A teleport with no interpolation | HIGH |
| `FUN_0021b228` | 1448 | `Node_SetMatrix` (named earlier; runtime-confirmed writer of the hero translation) | CONFIRMED |
| `FUN_00247bd8` | 5640 | `MoveSys_ExecuteActions` (kratos-data.md §6.5) | HIGH |
| `FUN_00284d00` / `FUN_00284cf8` | 8 | getters: `ms+0x30` (owner) / `ms+0x44` (current move data) | thunk |
| `FUN_001003b0` | 88 | for each child in list `+0x20`: call child vtable `+0x1c` (broadcast) | HIGH |
| `FUN_0021b7d8` | 700 | marks cells of an 11×11 grid (32-unit cells, origin `grid+0x30`, axes `grid+0/+0x20`, bitmask `grid+0x40`) within the character's radius (vtable `+0xcc` × 16) around its position (+8·vtable `+0xc4` upward when `+0x170 & 0x20`). MEDIUM: an occupancy grid (navigation/avoidance) | MEDIUM |
| `FUN_0021ba98` | 264 | countdown `+0x1e4 -= dt` while `+0x1e0` is set; at 0 plays a sound looked up from `0x2ee718` (string `SND_MINIGAME_AVAILABLE`) via `FUN_0017d758`; then `FUN_0021bba8` when `0x36a81c & 0x10000` | MEDIUM |
| `FUN_002233d0` | 164 | capsule from the character data `+0x3c0`: radii `+0x24`/`+0x28`/`+0x30`/`+0x34` × 16 × scale `+0x168` → `+0x120..+0x144` | MEDIUM |
| `FUN_0021c398` | 260 | release the held/grabbed object `+0x18c`: clear its links, send event `0x419` (`FUN_001d0fe0`) with a per-grab-type value, `FUN_0025ac00(2,0)`, clear `+0x174 bit 2` | MEDIUM |
| `FUN_001f21e8` | 208 | clear the lock-on/target: release `+0x32c` via `FUN_001b93a0`, reset `+0x324..+0x33c`, event `0x41c`, optionally set state 8 in `+0x170` | MEDIUM |

## 5. Movement collision

`FUN_00226af0` (move with collision) is documented in `docs/collision.md` §2 (2026-10-03).

## 6. Ground / air locomotion handler (`FUN_002284c8`)

First pass 2026-10-03, pass5 decompilation. This is the handler for the ground state (`0x3`); it also integrates air movement (`0x104c`) and ceiling hang (`0x800`). Field names below use byte offsets into the character object.

### 6.1 Fields

| Offset | Meaning | Level |
|---|---|---|
| `+0x50`, `+0x60`, `+0x70` | orientation axes (rows); `+0x70` is rewritten by the turn step | HIGH |
| `+0x80` | position (row 3 of the matrix) | HIGH |
| `+0xe0` | velocity (world units per second) | HIGH |
| `+0xf0` | one-frame impulse: added to the velocity, then cleared | MEDIUM |
| `+0x240` | ground normal (used as the slope factor) | MEDIUM |
| `+0x264` | current ground collider (0 = none) | MEDIUM |
| `+0x2e0` | current scalar move speed | HIGH |
| `+0x350` / `+0x354` | air-control ramp (0..1) and its ramp time | MEDIUM |
| `+0x358` | lean value, smoothed toward the turn amount (only with `+0x37c & 0x20000`) | MEDIUM |
| `+0x3c0` | character tuning block (see 6.3) | HIGH |

### 6.2 Steps

1. Clear the move speed `+0x2e0` in ground, ceiling and wall states.
2. **No stick input** (`param_5 == 0`), outside air states: zero the speed; apply **friction** `FUN_0021afa8(friction, dt, velocity)`. In air with bit 2, when the horizontal speed drops below `tuning+0x88 · 16`, enter a fall (`FUN_00225780(dt, this, 3)`).
3. **Stick input:**
   - stick magnitude clamped to 1, multiplied by `tuning+0x74` when `+0x374 & 2`;
   - speed profile from virtual `+0x124`: ~~`[0]` max speed~~ `[0]` **minimum** moving speed (corrected 2026-10-03, see 6.3), `[1]` target speed, `[2]` acceleration, `[3]` deceleration, `[4]` turn rate. All speeds are multiplied by 16 and the rates by dt. Kratos values in 6.4;
   - overrides: `+0x174 & 0x80` uses `tuning+0x1b8…+0x1c8`, ceiling hang uses `tuning+0x1d0…+0x1e0`, air uses `tuning+0x40` (acceleration) and `tuning+0x8c · +0x350` (air control);
   - speed approach `FUN_00221ad8(target, current, accel, decel, max)` writes `+0x2e0` (skipped when `+0x378 & 8`); on the ground the result is capped at 16 · target;
   - turn toward the stick direction: `FUN_00221be0(turn_rate · dt · 60, facing, desired)`, then re-orthonormalise the axes;
   - velocity = axes · speed, scaled by the ground normal on slopes, plus `+0xf0`. On the ground, an upward result starts a fall (`FUN_00225780(dt, this, 2)`).
4. **Gravity** unless the character is at rest on the ground: `vy -= g · scale · dt · 16`, with `g = DAT_002d86c8` (50.0 in the ELF data; it may change at runtime), `scale` = move system `+0x34`; negated on the ceiling; state `0x1000` multiplies by `tuning+0x98`. Terminal speed: `vy ≥ −tuning+0x38 · 16`.
5. **At rest** (`*param_6 = 1`): on the ground, no velocity, no input and the ground collider is not flagged `0x1000` in the table `DAT_0036a820`.
6. **Integrate**: `FUN_00221a88(dt·scale, this, offset)` returns position + velocity · dt + the pivot offset `+0x130` in local axes.
7. **Resolve**:
   - at rest: `FUN_00225aa8` (ground settle) or `FUN_002261a0`;
   - otherwise: `FUN_001f4820` (ledge/edge handling, when `+0x37c & 1`); then either `FUN_00226af0` (docs/collision.md §2, when `+0x374 & 0x4000`) or `FUN_00227048` (general collide-and-classify); then water `FUN_001f74c0` and `FUN_001faf50` when flagged.

### 6.3 Unit scale and friction

The code multiplies every tuning speed by 16 and uses 9.82 inside the friction formula. This gives **16 world units per metre** (HIGH; same factor as the particle speed ×16 in docs/particles.md).

`FUN_0021afa8(μ, dt, v)`: if `|v| > 1.6`, `v · clamp(1 − μ · 9.82 · 16 · dt / |v|, 0, 1)`; otherwise zero (Coulomb friction).

~~`FUN_00221ad8(target, current, accel, decel, max)`: clamp the target to ±max, zero it below 0.0001;~~
`FUN_00221ad8(target, current, accel, decel, min)` (corrected 2026-10-03 from the branchless selects): a
target below 0.0001 in magnitude becomes 0; otherwise its magnitude is raised to **at least** `min`
(`max(|target|, min)` with the sign kept). Then step the current value toward the target by `accel` when
speeding up and `decel` when slowing down, without overshoot. HIGH.

### 6.4 Kratos speed profile (RAM `ingame2`). CONFIRMED

Virtual `+0x124` is `FUN_0022cb10`: it walks the animation controller's moveset stack (`+0x3c4 → +0xea[]`,
depth `+0xee`) and returns the first moveset entry in the CRT table (`CRT+0x1c`, self-relative) that has a
profile. Kratos has six moveset entries (default and sub-weapons); all six hold the same profile:

| Index | Value | Use |
|---|---|---|
| `[0]` | 0.9 | minimum moving speed, m/s |
| `[1]` | 7.5 | target (full-stick) speed, m/s |
| `[2]` | 50.0 | acceleration, m/s² |
| `[3]` | 50.0 | deceleration, m/s² |
| `[4]` | 0.25 | turn rate per 1/60 s (`FUN_00221be0(rate · dt · 60, …)`) |
| `[5]`…`[8]` | 0.2, 30.0, 30.0, 4.5 | not read yet |

So Kratos reaches full speed (7.5 m/s) in 0.15 s and stops in 0.15 s. All tuning values are in
`analysis/kratos_tuning.tsv` (`tools/kratos_tuning.py`).

## 7. Animation controller update (`FUN_0023ad28`)

First pass 2026-10-03, pass5 decompilation. Callers: Character_Update, `FUN_001ae608`, `FUN_001b0470`, `FUN_00209188`, `FUN_002873d0`, `FUN_00287bf8`.

`FUN_0023ad28(rate, p0…p6, ctrl, active)` updates one **animation controller**. A character's controller is at `+0x3c4`. The seven floats are blend parameters (speed, direction etc.; see §3); `rate` is a playback time step.

### 7.1 Controller fields

| Offset | Meaning | Level |
|---|---|---|
| `+0x90` | owner character | HIGH |
| `+0x94` | state table: 8-byte entries `u16 flags, u16 next, char* remap` | MEDIUM |
| `+0x98` | main animation node (its `+0x20` list holds the layers) | HIGH |
| `+0x9c` | list of extra animation nodes, each with its layer id at `+0x38` (−1 = none) | HIGH |
| `+0xa4` | current state entry | HIGH |
| `+0xa8` | layer id of the main node | MEDIUM |
| `+0xac` | normalised clip time (0..1) | HIGH |
| `+0xb0` | external time source; when 0 the function returns the time in seconds | MEDIUM |
| `+0xb4` | current clip instance; `+0x38` → clip header (`+0x10` duration, `+0x14` frame rate), `+0x08` frame count | MEDIUM |
| `+0xd4` | time mode: 0 = none, 1 = time follows the `p5` parameter (distance-driven), else free-running | MEDIUM |
| `+0xd6`, `+0xd8`, `+0xda`, `+0xdc` | pending, current, requested and last state ids (`0xffff` = none) | MEDIUM |
| `+0xe0` / `+0xe4` | blend time; copied when a state starts | LOW |
| `+0xea` | output slot for the started move | LOW |
| `+0xfc`, `+0x100`, `+0x104` | one-shot blend parameters and node-type mask, sent once by `FUN_0023acd0` | HIGH |

### 7.2 Steps

1. `active == 0`: clear the state (`+0xa4`, `+0xac`, ids `0xffff`) and only run the blend update.
2. End time = frame count / frame rate of the current clip.
3. Advance time:
   - `rate ≥ 0`: `FUN_0023aaa0` advances every layer by `rate · fps` (`FUN_00100520` to `FUN_00106a40`); `FUN_0023ac10(0)` sets the layer speed (`FUN_00100408` to `FUN_00106740`);
   - mode 1: playback speed = `p5 / (duration / fps)`, so the cycle follows the distance moved (no foot sliding);
4. When the clip time passes the end, take the next state:
   - a requested state (`+0xda`) is remapped through the current entry's `remap` byte pairs (`FUN_0023ac98`, list ends at `0xff`);
   - otherwise the entry's `next` id is used when the time is ≥ 1;
   - the owner's virtual `+0x11c(owner, slot, id)` starts the move; the result is queued with `FUN_0024fea0(move system, move, 1)`; entry flag `4` starts the new clip at time 1.0 (reverse).
5. Blend update: `FUN_00100c20(p0…p6, node, 0x1e80e00)` for the main node and each extra node; this walks every layer that reports itself active (virtual `+0x4c`) and calls `FUN_00108d08`.
6. Send the pending one-shot parameters (`FUN_0023acd0`) and return the clip time in seconds.

### 7.3 Blend node types (`FUN_00108d08`)

Layer channels with flag `0x20` are blend nodes. The type is `flags & 0x1ec0e90`; mask `0x1e80e00` selects all types used by the controller.

| Type | Function | Parameters used | Level |
|---|---|---|---|
| `0x200` | `FUN_00108810` | p0, p2, p5, p6 | LOW (dispatch only; at least 4 children) |
| `0x400` | `FUN_00107c10` | **4-way directional blend** when the stick is fully deflected (`|p0|+|p1| > 0.99`): one child per axis is chosen by sign, weighted `|p0|` and `|p1|`. Shared phase advanced by speed p5 over the weighted stride (`duration · p6`) | MEDIUM |
| `0x800`, `0x400000` | `FUN_00107a30` | **1D blend**: N children spaced evenly over p0 ∈ [−1, 1] with triangular weights; one shared phase in layer `+0x4c`, advanced by `p5 / (duration · p6 / fps)` and wrapped at 1 (stride-matched) | HIGH |
| `0x40000`, `0x80000` | `FUN_00107ed8` | **pose scrub**: every child's time = (p0 + 1)/2 of its length; weights linear between the two neighbours of p1 ∈ [−1, 1] over N children. Flag `8` uses (p3, p4) instead of (p0, p1). Used for aim/lean poses | HIGH |
| `0x200000` | `FUN_00108040` | pose scrub by p0 as above; 4 children weighted by p1: p1 ≤ 0 mixes children 2/3, 0 < p1 < 0.5 mixes 2/1, p1 ≥ 0.5 mixes 2/0 | MEDIUM |
| `0x800000` | `FUN_001084b0` | **8-direction walk/run blend**: 4 walk + 4 run children (or 4 only); the speed p5 picks the mix between walk and run stride (`duration · p6`); weights from the signs and sizes of (p0, p1); each weight slews at most `4 · dt` per update; phase stride-matched. Only with stick magnitude² > 0.99 | MEDIUM |
| `0x1000000` | `FUN_001081b0` | **10-child walk/run blend**: per speed band one centre child (weight `1 − |v|·band`) and 4 direction children; 5 children = one band only; stick vector normalised when longer than 1; phase from child 0, stride-matched | MEDIUM |

All children of a node share one normalised phase. The child time is phase · fps, clamped to the clip length (`FUN_0027ef80`). Child fields: `+0x8` time, `+0xc` cleared, `+0x18` weight, `+0x38` clip header (`+0x10` duration, `+0x14` fps, `+0x18` slot index).

Types `0x400e00` also smooth the layer value `+0x50` toward p5 by 7.5 % per call; with no active node the layer `+0x4c` is cleared.

## 8. Jump, double jump, fall and landing

Status: first pass 2026-10-03 (pass7 decompilation plus the `ingame2` RAM capture for the tuning values).
The low 16 bits of `+0x170` hold the locomotion state (§1.1): `1` ground, `4` **jump (rising)**, `8` **air
(falling)**. Jumping and falling are not branches in the move data: the nav moves `MOV_Jump`, `MOV_VJump`,
`MOV_DblJump`, `MOV_VDblJump`, `MOV_Fall`, `MOV_HighFall`, `MOV_VFall`, `MOV_Land` and so on have no
branches (`analysis/dc/R_HERO00/moves.tsv`) and are selected by the code through the animation
controller's state id (`+0x3c4 → +0xda`). No code refers to their names or hashes. HIGH.

### 8.1 Functions

| Function | Reading | Level |
|---|---|---|
| `FUN_002267a8(dt, this)` **Character_TryJump** | see 8.2; returns 1 only for the ceiling drop | HIGH (structure), MEDIUM (vector components) |
| `FUN_002264d8(this)` **Character_StartJump** | clears the ground contact (`+0x240`, `+0x264`, `+0x270`) and calls `FUN_00284f70(this, 0, 0)`; clears the double-jump-used bits (`+0x174 & ~0x1000000`, `+0x378 & ~5`) and `+0x374 & ~0x200`; state = `4`; fall timer `+0x34c = 0`; animation state `2` (`MOV_Jump`), or `3` (`MOV_VJump`, straight up) when the horizontal speed component of `+0x2e0` is within ±16 (1 m/s); `+0x164 = tuning+0x78`. In water-surface mode (`+0x37c & 4`) it calls `FUN_001f8880` instead | HIGH |
| `FUN_00226600(this)` **Character_StartDoubleJump** | `+0x378 \|= 4` (double jump used), `+0x174 \|= 0x1000000`; state = `4`; animation state `0x17` (`MOV_DblJump`) or `0x18` (`MOV_VDblJump`) with the same ±16 test | HIGH |
| `FUN_00225780(dt, this, mode)` **Character_EnterFall** | clears the ground contact; mode bit 0 clear sets `+0x378 \|= 4` (no double jump after walking off a ledge); bit 2 sets `+0x374 \|= 0x200`. Releases the current move (`FUN_00250430`) when it does not match the locomotion moveset. While rising (state `4`) and without mode bit 1, it stays in the jump until `vy` drops below `−tuning+0x78·16`. Otherwise state = `8` and `+0x34c = 0`; in state `8` the fall timer `+0x34c += dt` once `vy < −(tuning+0x38 − 2)·16`. Animation state `6` (`MOV_HighFall`) when `vy < −tuning+0x3c·16`, else `4` (`MOV_Fall`) or `8` (`MOV_VFall`, no horizontal speed); `+0x164 = 0x551184e7` (about 1e13) | HIGH |
| `FUN_00225aa8(this)` **Character_Land** | state = `1`; clears air flags (`+0x374`, `+0x378`, `+0x174 & 0xfefffffe`); `+0x350 = 1.0` (air-control ramp reset); goes to `FUN_002262e8` instead when a move has flag `0x8000000` | MEDIUM |
| `FUN_002301d0`, `FUN_00254d28` | the same leave-ground sequence for two other character classes (tuning at `+0x2c8`, animation controller at `+0x2cc`; animation states 1/3/5) | MEDIUM |
| `FUN_00225720(this)` **Character_ApplyImpulse** | applies the pending impulse `+0xf0` to the velocity `+0xe0` (added, or replacing it when `+0x174 & 0x4000000`), then clears it and the bits `0x6000000` | HIGH |
| `FUN_002199a0(this, vec, replace)` **Character_SetImpulse** | stores the impulse at `+0xf0` and sets `+0x174 \|= 0x2000000` (`0x6000000` with `replace`); ignored when the move system or an active move has flag `0x80` at `+0x2b0`. Reached only through Character vtable `0x2f1440` slot `+0x5c` (`FUN_00285180`, skipped when `tuning+0xe & 4`) | HIGH |

### 8.2 `Character_TryJump` steps

1. If rising (state `4`) and `vy < tuning+0x88·16`, call `Character_EnterFall(dt, this, 3)`: the jump
   turns into a fall near the apex.
2. Return unless the jump request bit `+0x3b8 & 1` is set. The writer of this bit was not found in the
   static code (open question; a runtime write watch on Kratos `+0x3b8` will answer it).
3. Ceiling hang (`0x800`): unless the ceiling surface has flag `0x200` (`DAT_0036a820` table), drop:
   `Character_EnterFall(dt, this, 0)` with animation state `0x6e`; return 1.
4. Wall (`0x30`): wall jump `FUN_001f4348(this)`.
5. **Ground jump** is allowed on the ground (`1`), on a water surface (`0x80`), or in the air within the
   **coyote time**: state `8`, `+0x378 & 1` set and fall timer `+0x34c < 0.1` s. Every active move must
   also allow jumping (`(move+0x6c)->+4 & 8`). Then a component of `+0x2e0` is set to
   `tuning+0x78·16` (the launch speed); the player in water (`+0x170 & 0x80`) or with `+0x378 & 0x4000`
   triggers `FUN_00242808(+0x3dc, this)` (jump-out effect/sound); then `Character_StartJump` runs.
6. Otherwise **double jump** when `+0x37c & 0x100` (double jump available) and `+0x378 & 4` is clear
   (not used yet), while jumping or falling with `tuning+0x84·16 < vy < tuning+0x80·16`: the vertical
   velocity is zeroed, `tuning+0x7c·16` is added, `Character_StartDoubleJump` runs, and the move
   system's gravity scale is reset (`ms+0x34 = 1.0`, `ms+0x38 = 1.0`, `ms+0x3c = 0`).

### 8.3 Kratos values (RAM `ingame2`, tuning block at `0x00a96720`). CONFIRMED

| Tuning | Value | Use | Meaning |
|---|---|---|---|
| `+0x38` | 50.0 | terminal fall speed | 50 m/s |
| `+0x3c` | 40.0 | high-fall threshold | `vy < −40 m/s` |
| `+0x40` | 7.5 | air acceleration | |
| `+0x78` | 15.25 | jump launch speed | 15.25 m/s |
| `+0x7c` | 15.25 | double-jump launch speed | 15.25 m/s |
| `+0x80`, `+0x84` | 20.0, −100.0 | double-jump window on `vy` | −100 < vy < 20 m/s (almost always open) |
| `+0x88` | 2.0 | rise to fall switch | `vy < 2 m/s` |
| `+0x8c` | 0.05 | air-control factor (× ramp `+0x350`) | |
| `+0x98` | 0.1 | gravity factor in state `0x1000` | |
| `+0x24`, `+0x28`, `+0x30`, `+0x34` | 0.6, 2.2, 0.4, 0.4 | capsule sizes (`FUN_002233d0`) | metres |
| gravity `DAT_002d86c8` | 50.0 | | 50 m/s² (× move-system scale) |

Kratos had `+0x37c = 0x6bfffb` (bit `0x100` set: double jump available) in that capture.
With a launch speed of 15.25 m/s and gravity 50 m/s² at scale 1, a ground jump rises for about 0.3 s
and about 2.3 m (v²/2g) before the rise-to-fall switch; a double jump at the apex adds the same again.
This estimate ignores the move-system gravity scale (`ms+0x34`), which moves can change. MEDIUM.

### 8.4 Open questions

- Writer of the jump request bit `+0x3b8 & 1` (pad to controller path) and the physical button.
- Which component of `+0x2e0` receives the launch speed, and how the ground handler turns `+0x2e0` into
  velocity at the start of the jump (VU0 broadcast code; MEDIUM).
- `+0x164`: set per state (jump `tuning+0x78`, fall about 1e13, other states other tuning fields) and
  read as a speed normaliser `|v| / (+0x164·16)` by `MoveSys_ExecuteActions` and `FUN_0025fd80`.
  Meaning MEDIUM.
- Runtime check: a capture during a jump and a double jump.

## 9. Jumping from walls (`FUN_001f4348`, Character_WallJump)

First pass 2026-10-03 (pass7). Called by `Character_TryJump` (§8.2 step 4) when the state has `0x30`
(`0x20` wall climb, `0x10` the other wall state). The vector `+0x2e0` is the local move velocity; the
words compared below are its lateral component (word 0) and its wall-normal component (word 2, read
through `pexew`). Structure HIGH, component meanings MEDIUM. Kratos values from `analysis/kratos_tuning.tsv`.

**On a climbable wall (`0x20`), unless `+0x174 & 0x300`:**

1. **Gap jump** when one of the direction bits `+0x374 & 0x78` is set (`0x8`, `0x10`, `0x20`, `0x40`):
   animation state `0x25`, `0x26`, `0x27` or `0x28`, matching the four moves `MOV_ClimbALJump`,
   `MOV_ClimbARJump`, `MOV_ClimbAUJump`, `MOV_ClimbADJump` (left, right, up, down; the bit-to-direction
   order follows the move order and is MEDIUM). Sets `+0x174 |= 1`, `+0x374 |= 0x4080`, `+0x330 = FLT_MAX`,
   and resets the air-control ramp. Returns 1.
2. No jump when the lateral component exceeds `tuning+0xcc · 16 · 0.5` (Kratos: 0.75 m/s).
3. Stick toward the wall (normal component ≥ 0): only with `+0x174 & 0x400`, animation state `0x4a`,
   `+0x374 |= 0x4000`; returns 1.
4. Stick away from the wall: unless the wall surface (`+0x268`, table `DAT_0036a820`) has flag `0x80`,
   **kick off**: push away at `tuning+0xdc` (Kratos 10 m/s), turn round (`FUN_002266c8`), set the timers
   `+0x334` and `+0x330` to 0.25 s, mark `+0x378 |= 0x10`, launch upward at `tuning+0xd8` (Kratos 17 m/s),
   then `Character_StartJump`.

**Otherwise** (state `0x10`, or the wall state with `+0x174 & 0x300`): the launch speed is the normal
jump speed `tuning+0x78`.
- In state `0x10` with `+0x174 & 0x8000000`, the jump is cancelled and, on a surface flagged `0x2000`,
  `FUN_001f4248` runs instead (return 1).
- Stick away from the wall: unless the surface has flag `0x80`, turn round and jump (`+0x334 = 0.25`).
- Stick toward the wall: with `+0x37c & 0x10000` it runs `FUN_001f4248` (surface flag `0x400` blocks
  it); otherwise a short jump whose speed is twice the wall-normal component, clamped to 0…32 units/s, with
  `+0x334 = 5.0`.
- Ends with `Character_StartJump` (return 0).

Open: the exact meaning of state `0x10` and of `FUN_001f4248` and `FUN_002266c8` (turn round is MEDIUM).

## 10. Wall climbing, ladders, ledge hang and wall slide (`FUN_001f6000`)

First pass 2026-10-03 (pass7). The handler of the wall states (§3.1: `+0x170 & 0x30` with movement byte
`& 0x18`). It writes the local move velocity `+0x2e0` from the stick and the tuning block, then optionally
jumps (§8, §9) and resolves collision. Sub-modes are named from the hero moves that use them
(`analysis/dc/R_HERO00/moves.tsv`). Structure HIGH; which `+0x2e0` word is lateral / vertical / normal is
MEDIUM (VU0 broadcast code). Kratos values from `analysis/kratos_tuning.tsv` (RAM `ingame2`).

| Sub-mode | Condition | Movement | Kratos values |
|---|---|---|---|
| **Free climb** (`MOV_Climb`, `wallBlend`) | state `0x20`, `+0x174 & 0x700` clear | stick direction with speed `clamp(|stick|·16, tuning+0xc8·16, tuning+0xd0·16)`; a stick below 0.1 gives no motion. The gap bits `+0x374 & 0x20/0x40` and `& 0x8/0x10` suppress the lateral or vertical part (wall gaps, `MOV_ClimbA*`) | 0.5 to 2.0 m/s |
| **Ladder** (`MOV_Ladder`, `MOV_LadderU/D`) | state `0x20`, `+0x174 & 0x300` | fixed speed `tuning+0xc4` up or down by the sign of the stick | 1.25 m/s |
| **Wall slide** (`MOV_WallSlideEnter/Loop`) | state `0x20`, `+0x174 & 0x400` | vertical speed `tuning+0xe4` by the stick sign. When the stick is pulled down below `−tuning+0xe8` and the speed already exceeds 1 m/s downward, sliding starts (`+0x374 \|= 0x100`): it accelerates downward by `tuning+0xf0` up to `tuning+0xec`; otherwise it brakes by `tuning+0xf4` and leaves slide mode once slower than `tuning+0xe4`. On surfaces flagged `0x20`/`0x40` (`DAT_0036a820`) the slope of the surface feeds in | climb 2.0 m/s, stick threshold 0.9, slide accel 20 m/s², max slide 10 m/s, brake 40 m/s² |
| **Ledge hang** (`MOV_WallHang`, `MOV_WallHangL/R`) | state `0x10` | lateral only, fixed speed `tuning+0xc0` by the stick sign; `+0x378 \|= 0x1000` while moving right (cleared moving left). Not available with `+0x174 & 0x400` or `& 0x8000000` | 1.0 m/s |

**Common steps after the sub-mode:**
1. Wall-normal component (pull toward or away from the wall): `tuning+0xdc · stick · 16` toward the wall,
   `−tuning+0xdc · 16` away (only away when `+0x174 & 0x200`). Kratos: 10 m/s.
2. Clear `+0xe0` (velocity), then if jumping is allowed here (`+0x37c & 0x80`, not `+0x174 & 0x8000`) call
   `Character_TryJump`; when it returns 1 the position is integrated (`FUN_00221a88`) and the handler ends.
3. Velocity = axes · `+0x2e0`; clear the ground contact (`+0x27c = 0`, `+0x280 = −1`).
4. With `+0x374 & 0x4000` resolve with `FUN_00226af0` (docs/collision.md §2, push-out when `+0x374 &
   0x20000` is clear); otherwise, with `+0x37c & 0x40000`, `FUN_001faf50`.

`FUN_001f4248` (reached from the ledge-hang jump in §9) is most likely the pull-up (`MOV_WallPullUp`,
`ledgeHangPullUp`). MEDIUM, not read yet.

## 11. Swimming (`FUN_001f7cd8`)

First pass 2026-10-03 (pass7). The handler of the water states (`+0x170 & 0x380`, §1.1). Structure HIGH,
sub-state names MEDIUM (from the code paths and the moves `HitWater`, `DeathWater`, `DiveDashCollide`).
Kratos values from `analysis/kratos_tuning.tsv`.

| Bit | Reading | Level |
|---|---|---|
| `0x80` | swimming at the surface (slower target speed) | MEDIUM |
| `0x100` | in water, not diving; with `+0x374 & 0x2000` the vertical speed is kept | MEDIUM |
| `0x200` | **underwater** (free 3D swimming) | MEDIUM |

**Steps**
1. Rebuild the capsule when the scale `+0x168` changed (heights `tuning+0x28/+0x30/+0x34`; sets
   `+0x174 |= 0x100000` and resets `+0x158..+0x160`).
2. If the current move allows free motion (movement byte `& 0x18 == 0`):
   - turn toward the stick, with roll/lean `+0x358` smoothed by 0.15 per frame and clamped to ±1.2566 rad
     (72°), scaled by `tuning+0x194`;
   - horizontal speed: speed approach (§6.3) with target `tuning+0x184` (`tuning+0x188` at the surface,
     state `0x80`) × stick (× `tuning+0x74` when `+0x374 & 2`), minimum `tuning+0x180`, acceleration
     `tuning+0x18c`, deceleration `tuning+0x190`.
3. **Not underwater:** the vertical speed is braked to 0 (unless `0x100` with `+0x374 & 0x2000`). When the
   horizontal speed is below `(tuning+0x184 + 1)` m/s and the character is not on Pegasus, `Character_TryJump`
   runs: this is the **jump out of the water** (§8.2 step 5 allows state `0x80`). With state bit `4` the
   velocity is used as is; otherwise a vertical correction of `tuning+0x198 · dt` per frame pulls the
   character to the surface height.
4. **Underwater (`0x200`):** the action bits `+0x3b8 & 0x20` (up) and `& 0x40` (down) give a vertical
   target of ± the swim speed (the same input word as the jump request bit 0, §8.2). The total speed is
   capped at the swim speed. With `+0x374 & 0x800` vertical input is ignored and the velocity decays by
   `1 − 0.1 · scale`.
   - **Dive dash:** with `+0x374 & 0x1000` and `+0x34c ≥ 1`, the speed is `+0x360` instead, with
     acceleration and deceleration scaled by `+0x360 / swim speed`.
5. Integrate (`FUN_00221a88`), then ledge handling `FUN_001f4820` when `+0x37c & 1`, the capsule rebuild
   when leaving water, and the water-surface resolve `FUN_001f74c0` when its flag `8` is set.

| Tuning | Kratos | Use |
|---|---|---|
| `+0x180` | 1.0 | swim minimum speed, m/s |
| `+0x184` | 7.5 | swim speed (underwater / in water), m/s |
| `+0x188` | 4.5 | surface swim speed, m/s |
| `+0x18c` | 7.5 | swim acceleration, m/s² |
| `+0x190` | 7.5 | swim deceleration, m/s² |
| `+0x194` | 0.15 | turn/roll factor |
| `+0x198` | 15.0 | surface pull rate, m/s per second |

Open: the exact split of `0x100` and `0x200`; the dive-dash speed `+0x360` source; `FUN_001f74c0`
(surface resolve) and `FUN_001f8880` (water-surface jump variant called from §8.1).

## 12. Ropes (`FUN_001f3168`)

First pass 2026-10-03 (pass7), first part of the function only. The handler of state `0x400`.
Structure HIGH; the sub-mode names come from the hero moves `MOV_Rope`, `MOV_RopeU/D` (`ropeClimb*`),
`MOV_RSlide` (`ropeClimbSlide`) and `MOV_RopeJump` (`ropeHOHJump`, hand over hand) and are MEDIUM.

1. The rope object comes from the character's virtual `+0x3c` (vtable `0x2f1440`); without one the
   handler clears `+0x374 & 0x8000` and returns. Rope length = rope `+0x60`; the usable range ends 0.25
   before the end.
2. The character's place on the rope is the parameter `+0x324`, clamped to
   `[0.25 (0.5 in mode 0x10), length − 0.26]`. `FUN_001b9618(t, rope)` returns the rope point at `t`;
   the parameter is smoothed toward its new value each frame.
3. Sub-modes by `+0x174` (only when bit `0x40` is clear; the `0x40` path is not read yet):
   - **Climbing** (`0x10`, or sliding `+0x374 & 0x100`): the stick's vertical component gives the climb
     rate through `FUN_00221b78(stick · tuning+0xfc · 16, tuning+0xf8 · 16)`. With `+0x37c & 0x4000`,
     pushing the stick past 0.8 starts a **slide** (`+0x374 |= 0x100`): the speed rises by `tuning+0x110`
     per second up to `tuning+0x10c`; otherwise it falls by `tuning+0x114` per second and the slide ends
     at 0. The step along the rope is speed · dt / 16.
   - **Hand over hand** (`0x20`): when the current move allows it (movement byte bit 8) and the stick is
     pushed (> 0.25), the stick along the rope gives the speed through
     `FUN_00221b78(stick · tuning+0x11c · 16, tuning+0x118 · 16)`. Pulling back turns the character round
     (rebuild the axes, toggle the direction bit `+0x378 ^ 0x400`, `+0x174 |= 0x80000`); the direction bit
     sets the sign of the step.

| Tuning | Kratos | Use |
|---|---|---|
| `+0xf8`, `+0xfc` | 0.5, 1.5 | climb rate curve (arguments of `FUN_00221b78`) |
| `+0x10c` | 7.5 | maximum slide speed, m/s |
| `+0x110` | 20.0 | slide acceleration, m/s² |
| `+0x114` | 20.0 | slide braking, m/s² |
| `+0x118`, `+0x11c` | 1.25, 2.5 | hand-over-hand rate curve |

**Second half** (read the same day):
4. **Facing on a vertical rope** (neither `0x20` nor `0x40`, not sliding): the character snaps to one of
   four sides around the rope. The rope descriptor (`rope+0x38c`, byte `+0x3c`) gives the allowed sides
   (0: one side, 1: two opposite sides, 2 or more: all four). Pushing the stick sideways past ±0.5 steps to
   the next allowed side; each change starts a 0.1 s turn blend (`+0x150 = 0.1`, `+0x154`,
   `+0x174 |= 0x80000`). The axes `+0x50`/`+0x70` are taken from the chosen side.
5. In modes `0x20` and `0x40` the facing follows the rope direction at `t + 0.25`.
6. If the rope is still active (`(rope+0x384)->+4 & 4`) and the character is not past the end
   (`+0x324 < length − 0.25`): `FUN_001f2328`, then `FUN_001f2648` (rope actions and the rope jump; it
   calls `Character_StartJump`), then `FUN_001f2a08` places the character on the rope point. Otherwise
   the character drops off: `Character_EnterFall(dt, this, 2)`.
7. When the state is no longer `0x400` afterwards: release the lock-on target (`FUN_001f21e8(this, 0)`),
   start a 0.1 s blend and clear `+0x328`. Always clears `+0x374 & 0x8000` on exit.

Open: `FUN_00221b78` (rate curve), the meaning of mode `0x40` (it skips the stick handling; possibly
swinging or a scripted attach), `FUN_001f2328`/`FUN_001f2648`/`FUN_001f2a08` in detail.

## 13. Grapple swing (`FUN_001f8db0`)

First pass 2026-10-03 (pass7), first part of the function. The handler of state `0x8000` (grapple hang,
§1.1; moves `MOV_GrappleHangEnter/Idle`). The grapple runtime block is `tGrappleSystem` at `+0x3d0`
(`param_2[0xf4]`): `[0]` the grapple point object, `[2]` the **line length** (world units), `[3]` the
**swing speed**, `[4]` a mode value, `[8..]` the swing vector. The point's descriptor (`point+8`) has `[0]`
the point type and `[0xb]` an automatic reel speed. Structure HIGH; the point-type names are MEDIUM.

1. **Letting go** (when `+0x378 & 0x40000`):
   - jump request (`+0x3b8 & 1`): clear the motion, release the move (`FUN_00250430`), `Character_StartJump`
     with animation state `0x16` and turn round (`FUN_002266c8`): the jump off the grapple;
   - otherwise, when the grapple button is released (pad masks of the controller at `+0x1b8 → +4`,
     bit `0x10` falling edge of `+0x10` against `+0x18`): drop, with `+0x330 = 0.25` and `+0x174 |= 1`.
2. With mode `[4] == 0`, `FUN_001f9b58` computes the hang offset.
3. **Line length:**
   - point type 2: the line reels in automatically at `[0xb] · 16` per second;
   - otherwise, without the swing input (`+0x3b8 & 0x10`): the stick changes the length, clamped to
     `[tuning+0x210, tuning+0x214]` (Kratos 2 to 10 m). On points with sides (`desc+0x18`), pushing the
     stick sideways past 0.5 steps around the point (`FUN_001a05c0`/`FUN_001a06f8`, the same four-side
     scheme as ropes §12) with a 0.1 s turn blend.
4. **Swinging** (`+0x3b8 & 0x10` held): the swing speed approaches a target by 20 % per frame. For point
   type 1 (circular swing) the target is `√(length / 160) · 160 · tuning+0x200`. For other types it is
   `±√(length / 160) · 160 · tuning+0x1f8`, signed by the stick when the stick direction is within 75° of
   the swing plane (−75…5° or −5…75°), otherwise 0. 160 units = 10 m, so the speed grows with the square
   root of the line length like a pendulum.
5. For point type 1 the character's axes follow the swing (`FUN_001f8b40`), with a short blend near the
   point (< 2 m).
6. Animation state `0x72` when the line is pulled out to its maximum while reeling out; the clip end
   (state `0x71` at 99 %) gates the next step. The rest of the function (release, collision, the event
   `0x422` noted in §3.1) is not read yet.

| Tuning | Kratos | Use |
|---|---|---|
| `+0x1f8` | 9.0 | swing pump factor (pendulum points) |
| `+0x200` | 5.0 | swing factor (circular points, type 1) |
| `+0x20c` | −2.0 | read at entry (reel rate?), not settled |
| `+0x210` | 2.0 | minimum line length, m |
| `+0x214` | 10.0 | maximum line length, m |
| `+0x1f4`, `+0x1fc`, `+0x204`, `+0x208`, `+0x218`…`+0x228` | 1.5, 3.25, 3.25, 58.92, 0.4, 12, 12, 24, 24 | grapple block, not read yet |

The tuning block ends near `+0x22c`: from there the words are not floats (`analysis/kratos_tuning.tsv`).
