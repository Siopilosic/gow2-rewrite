# Runtime validation with PCSX2

Static findings (`confirmed.md`) are marked CONFIRMED from the executable alone. This document
describes how they are checked against the running game, and keeps a record of the results.

## Setup

| Item | Value |
|---|---|
| Emulator | PCSX2 **v2.8.2** (2026-09-04), `pcsx2-v2.8.2-windows-x64-Qt.7z`, SHA-256 `7DFC829CA1994CC1045AC49F05E39B6CF968B72E6A374C40E05C2A2B4AC200B4`, from github.com/PCSX2/pcsx2/releases |
| Location | `tools/pcsx2/` (portable mode: `portable.txt`; settings in `tools/pcsx2/inis/`) |
| IPC | PINE enabled in `inis/PCSX2.ini` (`EnablePINE = true`, slot 28011) |
| BIOS | must be dumped from your own console. Put it in `tools/pcsx2/bios/`. It is not part of the project |

## Procedure

1. Put the BIOS into `tools/pcsx2/bios/`.
2. Run `tools/pcsx2/pcsx2-qt.exe` and finish the setup wizard. Choose the BIOS, and add
   `D:\Decompiling God of War 2` as a game folder.
3. Check *Settings → Advanced → PINE* is enabled (slot 28011).
4. Boot `God of War II.iso`. At each capture point, **pause** the emulator (do not stop it):
   - `title`: the title/main menu.
   - `ingame`: in game, controlling Kratos.
5. For each capture point run:
   ```
   python tools/ramwalk.py pine snapshot <label>     # full 32 MB EE RAM -> analysis/runtime/<label>/ee.bin
   python tools/ramwalk.py analysis/runtime/<label>/ee.bin report <label>-report
   ```
   Alternatively, savestates (`F1`, written to `tools/pcsx2/sstates/`) can be read directly:
   `python tools/ramwalk.py <state>.p2s report <label>`.

Snapshots and reports are never overwritten: each label is a new folder under
`analysis/runtime/`.

## What `ramwalk.py report` checks

| Check | Model item |
|---|---|
| slot 0 is the root object (vptr 0x2f6968) | C-C1 |
| every non-null slot `i` holds an object with `(type_word>>16)&0xfff == i` | C-C1, C-C3 |
| the object's final vptr belongs to the server class `servers.tsv` assigns to `i` | C-C4/C-C5 |
| Master's child list is sorted by `order_key`, descending; `child_count` matches | C-C6 |
| pooled servers: bucket arrays, current bucket, context stack | C-C7, C-C8 |
| GO default bank has vptr 0x2f6530 | C-D6 |
| each GO context, its record name, `+0x70`, WAD, root node, node stack | C-D8, C-D9, H-O2, H-O6 |
| node trees: type word, flags, uid `+0x68`, matrix translations | C-D10 |

## Breakpoint checks (PCSX2 *Debug → Open Debugger*)

These test orders and call sequences, which a memory snapshot cannot show.

| Breakpoint | Expectation | Item |
|---|---|---|
| `0x00277118` Server_RegisterSelf | `a0` = new server; `g_ServerTable[(lw 0(a0) >> 16) & 0xfff]` written next | C-C3 |
| `0x0027f8f8` Mgr_UpdateChild | per frame, `a1` visits Master's children in list order | C-C6 |
| `0x0027cb48` GOServer_PushContext | `a1` = context built by `GOBank_NewObject` during load (`CXT_*`); every frame, from `0x00283510` | H-O2, C-D8 |
| `0x00140ff8` GoClassA4 slot 3 | hit once per active context per frame | C-D9, H-O6 |

## Results

### Capture `title` (2026-10-02 14:20, title screen, paused)

**Capture details**
- RAM: `analysis/runtime/title/ee.bin`, SHA-256 `5a1709112d215522…` (`meta.json`).
- Source: PINE, PCSX2 v2.8.2, serial `SCUS-97481`, version 1.01. A savestate of the same screen is `tools/pcsx2/sstates/SCUS-97481 (2F123FD8).01.p2s`.
- Integrity: all 482,761 `.text` words in RAM are identical to `SCUS_974.81`. The code runs unmodified, so static addresses apply directly.
- Reports: `analysis/runtime/title-report/report.txt` (first run, wrong record-name offsets) and `title-report2/report.txt` (names fixed). **60 checks OK, 0 mismatch.**

**Confirmed at runtime**

- **C-C1/C-C3 (server table):**
  - 26 non-null slots.
  - Slot 0 is the root object (vptr 0x2f6968).
  - Every other slot `i` holds an object with `(type_word>>16)&0xfff == i`.
  - The parent id in the low half is 5 (Master) for most servers and 0xd (renMaster) for 0x0f/0x11/0x17/0x1b/0x20, as the boot records say.
- **C-C4/C-C5 (classes):** each slot's vptr is the class `servers.tsv` predicts. The one exception, slot 2, is a correction (below).
- **C-C6 (Master):**
  - The 19 children are in descending `order_key` order, and `child_count` (+0x30) = 19.
  - That order is the per-frame update order: ProServer 7fff, WadServer 7f0f, AnimServer 7100, BhvrServer 7002, EvtServer 7001, CollisionServer 6200, ScriptServer 6000, GOServer 5f00, Effects/Waypoint/Camera 5e00, Light 5d00, Mat 5c00, Texture 5b00, GfxClut 5a00, Sound 5901, renMaster 5900, renPrimMaster 5700, EpiServer 0.
- **C-C7 (pooled servers):** one bucket each; the default bank is at index 0 with the expected default-bank vptr. GO default bank = 0x2f6530 (C-D6).
- **C-C8 / C-D8 (context stacks):** every pooled server has the same stack depth. GOServer's stack is `[CXT_R_Perm, 0x0176b6b8, CXT_R_Perm]`, and every other server shows the same A,B,A shape with its own objects. This fits `GOServer_PushContext` cascading the push to the component servers. What the 0x0176b… objects are is still open.
- **C-D9:** for all 7 contexts, node stack[0] (+0x2c) = the root node (+0x28), and the node-stack top is 0.
- **H-O2 (context architecture), now confirmed:**
  - The GO default bank holds 7 `GoClassA4` contexts (vptr 0x2f6610): `CXT_R_Perm`, `CXT_ScreenGO`, `CXT_R_Hero`, `CXT_R_ShellA`, `CXT_default`, `CXT_R_SubWeapon`, `CXT_t…`.
  - Each `+0x7c` points to a 0x54 descriptor whose `+0x34` is the record name and whose **`+0x18` points back to the context** (new fact, 7/7).
- **C-D10 (nodes):**
  - Children are 0x120-stride instances with type `0x40030001`, i.e. `go*` subtype 3 of server 1.
  - `+0x1c` is a 0x70 `go*` record object: `+0` type, `+4` pointer, `+8` name, `+0x20` matrix data, `+0x6c` link.
  - uids at `+0x68` are increasing. `+0x20` matrix translations hold world-like coordinates.
  - The second matrix (`+0x70`) is all zero for every node at this point.

**Corrected by runtime**

- `servers.tsv` gave renPrimMaster (id 2) the final vtable 0x2f44e8. At runtime its vptr is **0x2f3688**.
- The ctor stores 0x2f3688 at +0x20. 0x2f4520 and 0x2f44e8 are vptrs (at +0x60) of helper objects allocated inside the ctor (`Mem_New(0x64)`).
- Fixes:
  - `tools/server_profile.py` now uses only stores to +0x20.
  - `servers.tsv` and `wad_object_paths.tsv` were regenerated; the previous versions are kept as `*.v1.tsv`.
  - Only this row changed. Recorded as R11 in `hypotheses.md`.

**Leads for the Kratos stage (`project-goal.md`)**
- `CXT_R_Hero` (ctx 0x00913918, WAD context 0x00913488) holds `gohero` (node 0x006fd480), `gobubpart` and `goripples`.

### Capture `ingame1` (2026-10-02 14:32, Rhodes opening; the user had fought soldiers and taken one hit)

**Capture details**
- RAM: `analysis/runtime/ingame1/ee.bin`, SHA-256 `5cd4eccb…`. Report: `analysis/runtime/ingame1-report/report.txt`.
- **83 checks OK, 0 mismatch.**
- The server table, Master order and the classes are identical to `title`. renPrimMaster now matches the corrected `servers.tsv`.

**Context stacks**
- In game, every pooled server is back at depth 0 (or depth 1 for Light/Texture/Mat/GfxClut/renFlash, which have an extra base entry).
- The A,B,A stacks seen at `title` were therefore transient, captured during loading. B was 0x0176b6b8, which here is `CXT_R_Rhsold00`, i.e. the level was being loaded behind the title screen.

**GO contexts (29)**
- The persistent `CXT_R_*` set is `R_Perm`, `ScreenGO`, `R_Hero`, `R_SubWeapon`, **`R_Weapon`**, **`R_M_Earth`, `R_M_Medusa`, `R_M_Wind`, `R_M_Lghtn`** (one context per magic), `R_Rhsold00` (Rhodes soldier) and `R_Colsus00` (Colossus).
- The level is `rhod10`, with `CXT_default` plus per-area `CXT_bv*` contexts sharing WAD context 0x017dde20.

**`gohero` (node 0x006fd480)**
- It is the only child of `CXT_R_Hero` now; the title-screen children are gone.
- The `+0x20` matrix is a pure yaw rotation (cos 0.83 / sin 0.55 about Y) with translation **(-1704.28, 3712.00, -5353.45)** at `+0x50`. This is the candidate Kratos world transform; `ingame2` will test it.
- `+0x68` uid = 338598.
- `+0xb0`/`+0xb4` and the 13 slots `+0xc0..+0xf0` hold pointers. The `+0xc0` slots are candidate component pointers.
- `+0x104` = attachment 0x006fed20 (0xa0): identity 4×4 at `+0`, `+0x40` = the node uid, **`+0x48` = back pointer to the node**.

### Capture `ingame2` (14:33:54, 734 VBlanks after `ingame1`; the user walked straight, then turned right)

**Capture details:** RAM `analysis/runtime/ingame2/ee.bin` (SHA-256 `1de5011d…`). 83 checks OK, 0 mismatch.

**`gohero` node diff (`ingame1` → `ingame2`)**
- Only three things changed in the 0x120 node: the rotation (`+0x20`, `+0x28`, `+0x40`, `+0x48`), the translation x/z (`+0x50`, `+0x58`) and `+0x68`.
- The translation moved by (dx −29.91, dz −102.49), 106.76 units in the ground plane. y stayed at 3712.00 (flat floor).
- Yaw went from 33.6° to −66.0°.
- Conclusions:
  - **CONFIRMED (runtime):** node `+0x20..+0x5f` is Kratos's world transform. It is a row-major 4×4, row 3 is the translation, and Y is up (the rotation stays about Y).
  - Which sign is "right" depends on the handedness convention, which is still open. One more capture turning only left would settle it.
- **`+0x68` is not a fixed uid.** It is a stamp taken from the global counter `0x002d7978`, which grew from 338,617 to 373,575 (≈47.6 per VBlank). The stamp is refreshed when the matrix is written. Static C-D10 called it "uid"; corrected here. Many matrix routines (`0x0010ba18…`) bump that counter. Attachment `+0x40` mirrors the stamp.

**Objects the hero node points to** (diffed over 0x400 bytes)

| Node field | Object | Changed | Observation | Confidence |
|---|---|---|---|---|
| `+0xc0` | 0x0076a9d4 | 26 words | a full rotation + translation matching the hero position, offset by +31.9 in y | MEDIUM: a skeleton joint / root-bone world matrix |
| `+0xcc` | 0x00954cdc (`+0x20` → node) | 40 words | several points near the hero, each followed by a radius-like float (47.1, 6.0, 6.0) | MEDIUM: collision sphere set |
| `+0xe4` | 0x00954b74 | 51 words | copy of the node matrix at `+0x5c`, plus more vectors | LOW: movement/physics state |
| `+0xec` | 0x00954e54 | 12 words | one point near the hero and one far point (~20 000 units away); counter 0x50 → 0x18 | LOW: a ray or look target |
| `+0xb4`, `+0x10c` | | | the word 0x277f → 0x2a5d (10111 → 10845) appears in several places; Δ = 734 = the VBlank Δ | HIGH: a per-frame stamp of the last update; the game updated every VBlank between the captures |

**Node message handler (static, prompted by the capture)**
- Node `+0x108` held 0x001d6f28, a code address. `+0x10c` held 0x00711dc0, an object whose `+0x20` points back to the node.
- `FUN_0013d350(node, fn, ctx)` sets the handler: `+0x108 = fn`, `+0x10c = ctx`.
- `FUN_0013d360(node, &ctx)` gets it.
- `FUN_0013d378(node, msg)` **dispatches** it: `if (fn) fn(node, msg, ctx)`.
  - Its callers are `GoClassA4` vtable slots 23/24/25 (0x00140f98, 0x00140bb0, 0x00140af8) and `FUN_00121b18` (EvtServer code range).
- `FUN_001d6f28` itself handles message ids 0x79/0x7a/0x7b (`*(s16*)msg`). On 0x7b it clears the handler. It is installed by 8 functions in 0x001ce000–0x001d7600.
- This supports H-O6: behaviour reaches nodes through **messages and handlers**, not vtables. Whether the hero's main movement is driven this way is still unknown; the movement writer is not found yet.

### Breakpoint `bp-hero-pos` (write watch on `0x006fd4d0`, hero translation x)

**Hit details**
- It hit at once, with Kratos **standing still** (the user confirmed no stick input).
  - The position is rewritten every frame, even when idle.
  - The stack below is therefore the idle per-frame path. Whether the same path runs while Kratos walks is untested.
  - To test it: enable the watch while holding the stick, then compare the stack.
- **Walking hit (same session):** the watch was re-enabled while the user held the stick.
  - Same `pc` 0x0021b7b8, same `ra` 0x0022aa5c, and the stack is identical in all 17 frames.
  - Some registers differ from the idle hit (`t6 = 0x2000`, `t7 = 0x87`, `lo`).
  - Conclusion: walking and idle use the same per-frame write path (CONFIRMED for the first write of the frame).
  - Second-writer check: the user continued 3 more times while walking.
    - Every stop had pc 0x0021b7b8, ra 0x0022aa5c and the identical 17-frame stack.
    - The data differed each time (`s0` = 3F404F66 / 3F609A9C / 3EF1DDDA; `t7` = 0x361 / 0x30A / 0x2B4).
    - **CONFIRMED: while walking, `Character_Update` → `Node_SetMatrix` is the only writer of the hero translation.**
    - Not yet checked for other states: combat, jumping, grabs, cutscenes.
- `pc = 0x0021b7b8`: `sq v0, 0x50(a1)` inside `FUN_0021b228`, with `a1 = 0x006fd480` (`gohero`).
- `ra = 0x0022aa5c`, so the call came from `0x0022aa54`.
- `sp = 0x01ffebd0`, `s2 = 0x00958160`.

**`FUN_0021b228`**
- A shared routine that writes a node's 4×4 matrix (rows `+0x20…+0x50`).
- It also stores `a0` to `g_MatrixStamp` (`sd a0,0x7978(a2)`) and the stamp to node `+0x68`.
- It has 20 static call sites.

**`FUN_00228f90`** (11,012 bytes, 421 callees, string `"PegCamera01"`)
- It has no direct callers; it is **slot 2 of vtable 0x002f1440** (38 slots).
- That vptr is stored at **`+4`** by `FUN_00224af8` and `FUN_00224c30`, a class hierarchy separate from the engine `PolyObject` (vptr at +0x20).
- `FUN_00224af8` calls base ctor `FUN_0021cc38(this, a1, a2)` and clears `+0x37c`/`+0x3b0..+0x3b8`; the object is ≥ 0x3bc bytes.
- It is constructed from `FUN_001ff3b8`, `FUN_0020bb08` and `FUN_00231a48`.

**Live instances of vtable 0x2f1440** (RAM scan of `ingame2`, word 0x2f1440 at `obj+4`)

| Object | `+0` (node) | `+0xc` (WAD context) |
|---|---|---|
| 0x00958160 | 0x006fd480 `gohero` | 0x00913488 (`CXT_R_Hero`) |
| 0x01b49d40 | another node | 0x014cbbf0 (`CXT_R_Colsus00`) |
| 0x01b4a560, 0x01b4ad80, 0x01b4b190, … | other nodes | 0x0176b228 (`CXT_R_Rhsold00`) |

**Call stack at the hit** (PCSX2 *Stack* tab), outermost first. Every frame is runtime-observed.

| # | Function | Return PC in frame | Identification |
|---|---|---|---|
| 1 | `_start` 0x00100000 | 0x001001c8 | |
| 2 | `main` 0x001463b8 | 0x00146430 | C-B3 |
| 3 | `Engine_MainLoop` 0x00187570 | 0x00187598 | C-B5 |
| 4 | `Engine_Frame` 0x00187540 | 0x00187550 | C-B5 |
| 5 | `Engine_UpdateServers` 0x00187388 | 0x001873fc | C-B5 |
| 6 | `Mgr_UpdateChildren` 0x00277200 | 0x00277280 | C-C6 (Master) |
| 7 | `Mgr_UpdateChild` 0x0027f8f8 | 0x0027f914 | C-C6 |
| 8 | **EvtServer slot 3** 0x001218d8 | 0x00121a08 | vtable 0x2f5178[3]: **EvtServer_Update** |
| 9 | 0x0011dbb0 | 0x0011dbe4 | direct call from 8 |
| 10 | 0x0011dcd0 | 0x0011dd7c | direct from 9 |
| 11 | 0x0011de30 | 0x0011de84 | direct from 10 |
| 12 | 0x0011d688 | 0x0011d6e8 | indirect; vtable 0x2f50b8 slot 2 (9-slot class, ctors 0x0011d4c0/0x0011d5e0) |
| 13 | 0x002a4d18 | 0x002a4d44 | indirect; no static reference found yet |
| 14 | 0x0029d1f8 | 0x0029d24c | indirect; no static reference found yet |
| 15 | 0x00215cb0 | 0x00215ecc | direct from 14; returns early unless `0x002d8dcc != 0` and mode `0x002d8ec0` is 0xb/0xe (or `0x002d8ed8`) |
| 16 | `Character_Update` 0x00228f90 | 0x0022aa5c | vtable 0x2f1440[2], `this` = 0x00958160 (Kratos) |
| 17 | `Node_SetMatrix` 0x0021b228 | 0x0021b7b8 | the write |

What this shows:
- Frames 1–7 confirm the static boot/main-loop/Master chain (C-B3, C-B5, C-C6) at runtime.
- **The hero is updated inside EvtServer's frame** (Master order 0x7001), not inside GOServer's. This agrees with H-O6.
- Frames 12–14 are dispatched through function pointers; they are the next static targets.

So 0x2f1440 is a **character class shared by Kratos and the enemies**. Its slot 2 runs every frame and ends by writing the node transform. Confidence: HIGH from one hit plus the instance scan; who calls slot 2 is not traced yet.

**Next runtime step (superseded):** set a PCSX2 memory **write** breakpoint on `0x006fd4d0` (hero translation x) and move Kratos. The hit PC is the code that moves Kratos. This only holds while the hero node stays at the same address; re-check with `ramwalk.py pine go` after loading.
- `CXT_R_SubWeapon` (0x0076e210) exists at the title. It has no nodes yet.
- `CXT_default` (the shell level, `active=0x7b`) holds the scene nodes (`gostart`, torches, rooms, …).
