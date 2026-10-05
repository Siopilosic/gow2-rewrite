# Camera: the camera manager, zone selection and blending

Status: first pass 2026-10-03 (pass7 decompilation, RAM capture `ingame2`). Structure HIGH unless
stated; most field meanings MEDIUM. Confidence levels follow `docs/confirmed.md`.

## 1. Objects

| Object | Reading | Level |
|---|---|---|
| CameraServer (server id `0x09`, vtable `0x2f2ad0`) | a generic pooled server: its slots are the shared `PooledServer_*` functions. It holds the level's `CAM_` records (`docs/formats.md`) | HIGH |
| **camera manager** `DAT_002d8dcc` (RAM `ingame2`: `0x00729bb0`) | built by `FUN_002328b0` (string `"The_Viewport"`); one per game | HIGH |
| `The_Viewport` | the render viewport found by name; manager `+0x0c` | HIGH |

**Camera manager fields** (from `FUN_002328b0`):

| Offset | Meaning | Level |
|---|---|---|
| `+0x04`, `+0x06` | u16 mode words (both 1 at creation; `+4 == 0` makes the zone list restart, `+6 == 3` skips the roll/pitch step) | MEDIUM |
| `+0x08` | the camera's scene node (built from `DAT_002fd4e0`, registered with the CameraServer); its matrix is the view | HIGH |
| `+0x0c` | `The_Viewport` (`+0x3b4 = 0x45`) | HIGH |
| `+0x10` | **camera shake** system (`FUN_00285918`); `tActionCameraShake` and `SCR_CameraShake` feed it through `FUN_00234f60(manager, preset)` | HIGH |
| `+0x98` | **camera sway** system (`FUN_00285940`, `SCR_CameraSway`) | MEDIUM |
| `+0x90`, `+0x94` | pools: 8 entries of 0xe0 bytes, 64 of 0x2c bytes | MEDIUM |
| `+0xd0` | the focus point: the player's position from `FUN_002166b0` when a player exists | MEDIUM |
| `+0x144` | name of the current camera (initialised to `"None"`) | HIGH |
| `+0x174` | `DAT_002d7ea8` (render camera block; receives the final view matrix) | MEDIUM |

## 2. Per frame (`FUN_00234610`, called through `FUN_00234e48`)

1. Unless the camera is held (`+0xf0` set, or no camera and `FUN_001d1188` false), **select and blend**
   with `FUN_002336e8` (§3).
2. `FUN_002340b8`: the shake/sway/offset layer (not read in detail).
3. Build the view: optional roll and pitch offsets (`DAT_002d8d70`/`DAT_002d8d74` in degrees, scaled by
   `DAT_0032f3f4`/`DAT_0032f3f0`) unless the camera's two axis modes (`+0x1c`, `+0x1e`) are 0 or 4; a
   rotation about `DAT_002d8d78`; the result goes to the camera node's matrix (`+0x20…+0x5c`, bumping
   `g_MatrixStamp`) and to the render camera block.
4. **Aspect ratio:** 4:3 (1.3333) normally; `DAT_00363810 / 1.3333` when the widescreen flags
   `DAT_002d8dc0`/`DAT_002d8dbc` are off, blended with `+0x44` (`FUN_001777c0` sets the viewport).
5. Effects such as `FSE_UnderwaterEffect` when the camera is under water.

## 3. Zone selection and blending (`FUN_002336e8`, 2,508 bytes). MEDIUM

- **Active camera list** `DAT_003392a8`, count `DAT_002d8d90`, entries of 0x1c bytes:
  `{camera object, …, blend-in, blend-out, blend time (from the camera +0x54/+0x58/+0x5c), priority (+0x60)}`.
  Cameras become active when the focus point is inside their zone (the list is refilled elsewhere; open).
- Each camera's flags (`+0x18`) are tested against the current mask from `FUN_00233130`; a camera that
  fails gets priority `0x80000000` (never chosen).
- **Per-transition overrides:** a camera has a table (`+0x8c` count, `+0x90` entries of 0x28 bytes) keyed by
  the *previous* camera's name (compared with manager `+0x144`); a match replaces the three blend values,
  and a non-zero `+0x24` in the entry forbids the transition.
- The **highest priority** wins. The blend stack `DAT_00339628` (count `DAT_002d8d94`, entries of 0x1c)
  keeps the cameras being blended; entries whose camera is no longer active are removed.
- `FUN_00232c38(manager, camera)` switches the current camera (4 callers, also from scripts).

So the in-game camera is level-authored: zones with fixed or rail cameras (`CAM_` records, and the rail
parameters `CamOffsetX/Y/Z`, `Speed`, `Curvature`, `PopToNext`, `CamRadius` read by `FUN_001ab778`),
chosen by priority around Kratos and blended by time. There is no free orbit camera for normal play.

## 4. Level camera data: `CAM_` cameras and `CMZ_` zones (2026-10-03)

`tools/cam_dump.py <wad>` writes `analysis/cameras/<WAD>/cameras.tsv`, `transitions.tsv` and `zones.tsv`.
RHOD10: 46 cameras, 7 zones.

### 4.1 `CAM_` record (CameraServer `0x09`, WAD param 26). HIGH (layout), CONFIRMED (RAM `ingame2`)

`FUN_0014bf10` names the runtime camera class `"RCM_%s"` (so `CAM_CamBG21` becomes **`RCM_CamBG21`**, the
name at manager `+0x144`). `FUN_0014c060` builds the class: it copies 100 bytes from record `+0x80` to class
`+0x10` (class offset = record offset − 0x70), looks up the rail, and copies the transition table.

| Record | Class | Meaning | Level |
|---|---|---|---|
| `+0x00` | — | u32 9 (server id) | CONFIRMED |
| `+0x10..+0x4c` | — | the node matrix: rotation rows, position at `+0x40` | CONFIRMED (RAM: camera node `0x01c8f9d0` holds the same matrix) |
| `+0x50` | `+0x94` | rail curve name (`NCV_RailBG11` …), resolved by `FUN_0014b9d0` | HIGH |
| `+0x68` | `+0x98` | second name (empty in RHOD10) | MEDIUM |
| `+0x80` | `+0x10` | 0.95 (class default `0x3f733483` = 0.95 when built without a record) | MEDIUM |
| `+0x88` | `+0x18` | flags (tested against the mask of `FUN_00233130`, §3); `0x100` = dynamic camera, `0x10000` = fixed target point, `0x40000` = target offset from the node, `0x80000`/`0x100000` change the focus distance | HIGH (tests), MEDIUM (meanings) |
| `+0x8c`, `+0x8e` | `+0x1c`, `+0x1e` | two axis modes, values 0–3; 3 uses the rail (`FUN_002374b8` finds the rail position) | HIGH (values), MEDIUM (meanings) |
| `+0x94/+0x98`, `+0x9c/+0xa0`, `+0xb4/+0xb8` | `+0x24/+0x28` … | min/max pairs (the constructor clamps each min to its max) | HIGH (pairs), LOW (meaning) |
| `+0xa4`, `+0xa8` | `+0x34`, `+0x38` | angle limits in degrees (90 and 0–180); below 90/180 the solver limits the turn | MEDIUM |
| `+0xc8`, `+0xcc` | `+0x58`, `+0x5c` | blend-in / blend-out seconds (0.45 / 0.55 typical) | CONFIRMED (RAM active-list entry) |
| `+0xd0` | `+0x60` | priority (0–60) | CONFIRMED (RAM: 2 for `RCM_CamBG21`) |
| `+0xd4..+0xdc` | `+0x64..+0x6c` | target offset in metres (×16 at use) | HIGH |
| `+0xe4` | `+0x8c` | transition count | HIGH |
| `+0xe8 + 0x28·i` | `+0x90` | transition: `char[24]` previous camera (`RCM_…`), time, blend-in, blend-out, forbid flag | HIGH (matches §3) |

### 4.2 Per-frame camera pose (`FUN_002392b0`). HIGH (dispatch), MEDIUM (solver)

1. **Target** (`instance+0xa0`): Kratos's focus point (`FUN_00239ea0`), or a fixed point (flag `0x10000`),
   or the node position plus the record's offset (flag `0x40000`, static cameras only).
2. **Pose**: `PegCamera01` → `FUN_001b2a58` and `PegProjCam` → `FUN_001b2938` (the Pegasus cameras);
   a camera without flag `0x100` keeps its authored node matrix and only computes yaw, pitch and distance
   to the target (`FUN_002359f8`, used by the blend); with flag `0x100` the **dynamic solver**
   `FUN_00237e28` (5,244 bytes; axis modes 0–3, angle limits, rail) moves and turns it. MEDIUM: the
   solver's maths is not read line by line.
3. **Focus distance** (`instance+0xd8`): `|camera − target| × (1 − f) / 16`, `f` = class `+0x50` with
   flag `0x100000`, else a global; clamped unless flag `0x80000`.

In RHOD10 most gameplay cameras are dynamic (`0x433fe`, `0x437ff`, `0x2433fe`), many with axis mode 3 and
their own rail (`NCV_Rail*`); `CAM_CC*` (cutscene-style, position 0) and `CAM_CamPP20` have no rail.

### 4.3 `CMZ_` camera zone (WAD param 8). HIGH (layout)

Header: u16 0x10, u16 2, `"mCDbgHdr"`, u32 size, then offsets: camera names (`char[32]` each, the
`RCM_` names), per-volume vertex counts (u16, padded to 4 bytes), per-volume index counts, vertices
(xyzw floats), u16 edge indices (24 = 12 edges per volume), one centre per volume. Every volume in RHOD10
is an 8-vertex block 128 units (8 m) tall. Example `CMZ_gosbeginningentcams`: cameras `RCM_CamBG20/21/22`,
6 volumes.

**RAM check.** In `ingame2` Kratos stands at (−1734.2, 3712.0, −5455.9). That point is inside volume 2 of
`CMZ_gosbeginningentcams` and volume 2 of `CMZ_gorhod10cams`, and the current camera is `RCM_CamBG21`,
one of the first zone's cameras. CONFIRMED that zones select cameras. Open: the volume → camera mapping
(volume and camera counts differ: 17 volumes, 21 names in `CMZ_gorhod10cams`), and why `RCM_CamBG22`
(priority 25) did not win (flag mask or a forbidden transition, not checked).

### 4.4 `NCV_` rail curve (WAD tag 9). CONFIRMED (code + continuity)

`u32 n` segments, 12 bytes padding, `n` × 4×4 floats, then `n` knots (1, 2, 3, 4 in RHOD10).
`FUN_0014b9d0` resolves the name and sets the curve's segment pointer (`+0x10`) and knot pointer.
Segment `i` covers the **global** parameter `t ∈ [knot[i−1], knot[i]]` (knot[−1] = 0):
`P(t) = [t³ t² t 1] · M_i`, divided by its w (rows 0–2 have w 0, row 3 w 1). Evidence: `FUN_0014ba20`
(closest point on the rail to a target: 16 samples per segment, 17 on the last) builds exactly this vector,
and with the global parameter consecutive segments meet within 0.02 units on every RHOD10 rail (with a
local 0..1 parameter they do not). RHOD10 has 19 rails, 4 segments each.

**Viewer.** `tools/cam_dump.py` also writes `analysis/levels/<WAD>_gltf/cameras.json`; the level viewer's
"cameras" checkbox draws the camera positions (yellow), the rails (cyan) and the zone volumes (magenta) over
the level. In RHOD10 the zone volumes follow the walkway around the arena wall and the rails run along it.

### 4.5 The dynamic solver (`FUN_00237e28`). MEDIUM (structure read; maths not checked against a capture)

Record offsets below (class = record − 0x70). Units: distances in metres, ×16 at use.

1. **Framing (dead zone).** The target is turned into the camera's screen angles (instance `+0xb8/+0xbc`)
   and distance (`+0xc0`). The allowed screen box is `atan(k · tan(fov/2) · v)` for `v` = `+0xac/+0xb0`
   (vertical, `k` = 0.75, the 4:3 aspect) and `+0xb4/+0xb8` (horizontal). If the target leaves the box the
   angles are clamped and the camera re-aims (`FUN_00235960`); inside it the camera does not turn. Most
   RHOD10 cameras use −0.15/−0.15 and 0/0: Kratos kept centred horizontally and a little below centre.
   Flag `0x400000` skips the box (always re-centre).
2. **Axis modes** (`+0x8c`, `+0x8e`):
   - **3, rail:** the rail parameter (instance `+0x5c`) moves towards the best parameter found by a
     1-D search (`FUN_00276020/276298/276510/276788`, tolerance 0.0015; the variant depends on flags
     `0x200`, `0x1800000` and on the angle limits), at most a quarter segment per frame, smoothed:
     `t += (1 − s) × (t_best − t)` with `s` = `+0xbc` (0.913 in RHOD10, so 8.7 % of the gap per frame).
     Closed rails (curve flag `+0x04 & 1`) wrap. The camera sits at `rail(t)`.
   - **2, distance:** the camera moves along its view line so the distance to the target is
     `d × (1 − +0x90)`, clamped to [`+0x98`, `+0x94`] (0 … 1000 m).
   - **0 and 4, player orbit:** yaw and pitch change with the right stick (`DAT_002d8dc8`, `DAT_002d8db4`,
     `DAT_002d8db0` × speed `DAT_0032f3f4/f0` × frame time × 60), pitch clamped to ±90°; only while
     manager `+0x04` is 0 and `+0x06` ≠ 3.
   - **1:** no positional rule found (the camera keeps its position and only turns). LOW.
3. **Distance clamp** to [`+0xa0`, `+0x9c`] (0 … 14 m), mirrored when `+0x90` < 0.
4. **Turn limits:** yaw within ±`+0xa4` and pitch within ±`+0xa8` degrees of the camera's authored direction
   (`FUN_00235398` clamps; 90 and 33.8 for `CAM_CamBG21`).

So a typical RHOD10 gameplay camera slides along its rail to keep the best view of Kratos, with heavy
smoothing, and turns only when he leaves a small screen box, within ±90° yaw and a limited pitch of its
authored direction. There is a stick-controlled orbit mode (0/4), unused by RHOD10's cameras.

## Open questions

- ~~The `CAM_` record layout and the camera types (fixed, rail, look-at); `FUN_00237e28` (5,244 bytes,
  called from `FUN_002392b0` with `PegCamera01`/`PegProjCam`, the Pegasus camera).~~ Layout and dispatch
  done (§4). Still open: ~~the dynamic solver `FUN_00237e28` in detail, the axis-mode meanings~~ structure read (§4.5);
  the four rail-search variants and axis mode 1 are open.
  ~~The rail curve (`NCV_`) format.~~ Done (§4.4).
- ~~How the zone list is filled (zone volumes; `CDZ_`/`VSZ_` records of the CollisionServer are candidates).~~
  The zones are `CMZ_` records (§4.3). Still open: the volume → camera mapping and the code that tests them.
- `FUN_002340b8` (shake/sway composition), the shake presets `CSH_*` in `R_PERMA`.
- ~~A runtime check: the current camera name at `manager+0x144` in a capture.~~ RAM `ingame2`: the
  current camera is **`RCM_CamBG21`**; the stale active-list entries hold priorities 1, 1, 2 and blend
  values 0.45 and 0.55 (seconds, MEDIUM). Camera objects are named `RCM_*` (also `RCM_PegProjCam`):
  ~~the `RCM_` record family and its server are the next thing to identify.~~ `RCM_` is the runtime name of a
  `CAM_` record (§4.1).
