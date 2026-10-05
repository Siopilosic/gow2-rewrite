# Research status

_Last updated: 2026-10-02 (phase 2: verification and semantic reconstruction of the engine core)._

## Environment and inputs

| Item | Value |
|---|---|
| Target | God of War II, NTSC-U v1.01, `SCUS_974.81` |
| `SCUS_974.81` SHA-256 | `FD403603BE46DEA70D130695820D28B3F31DE6D7E48484A5F006398A605F7DD6` |
| `GODOFWAR.TOC` SHA-256 | `9B339A2DFDA12827D221E173A2FB806D7BE5D6C436BCFFCC650E1486604D77B0` |
| Ghidra | 12.1.4 PUBLIC, build 2026-Sep-21, revision `8b6bbb857accdfa20dc5b2f5dea471178c2e9fbc` |
| Processor module | ghidra-emotionengine-reloaded **v2.1.37**, release asset built for Ghidra 12.1.3, `extension.properties` version patched to 12.1.4; zip SHA-256 `3AF7641174B470BF19EC32B72B3929DBD21EC3F11D66894D2316BE044FAC1258`; language `r5900:LE:32:default` |
| Java | 21.0.12 |
| Independent disassembler | `tools/eedis.py` (Capstone 5.0.7 MIPS64 + hand-decoded R5900 lq/sq/MMI/COP2), Python 3.14.7 |
| Rust | stable via rustup (workspace `gow2-rs`) |

## Analysis passes

| Pass | What | Output | Notes |
|---|---|---|---|
| pass1 | import + default auto-analysis + export | `analysis/exports/pass1/` (TSV/CFG only) | **The pass-1 decompiler `.c` files were deleted** by the first version of `run_ghidra.ps1`, and the pass-1 Ghidra database was replaced by pass 2 (`-overwrite`). Everything else of pass 1 is in `snapshots/2026-10-02_pass1/`. Pass 1 can be reproduced by re-importing without `EnableParamId.java`. |
| pass2 | re-import with *Decompiler Parameter ID* + *Aggressive Instruction Finder*, 15 symbols | `analysis/exports/pass2/` | 5,883 functions; Ghidra project snapshot `snapshots/2026-10-02_pass2_ghidra/` |
| pass3 | `ApplyCore.java` on the pass-2 DB: 516 functions at vtable targets, 13 structures, 118 names, 37 signatures | `analysis/exports/pass3/` | 6,403 functions |
| pass4 | + volatile blocks for interrupt-written globals, VBlank/GS handler names | `analysis/exports/pass4/` | current decompilation; `unreachable_classification.txt` |
| pass5 | final symbol set of this phase (see below) | `analysis/exports/pass5/` | the database in `ghidra/` is at this state |

`tools/run_ghidra.ps1 -Pass <label>` never overwrites an earlier export. Snapshot `ghidra/`
before using `-Reimport`.

## Confirmed discoveries (summary; details and validation in `confirmed.md`)

1. WAD record format confirmed from the executable side: header written field by field by the
   boot record writer, stride computed by the dispatcher, tag 0 has no payload (C-A3, C-A4).
2. Complete WAD tag → handler table (C-A5).
3. TXR (88 bytes) and instance-record (48 bytes) payload layouts confirmed by reader and disc
   (C-A7, C-A8); group semantics confirmed on all 93,041 groups on the disc (C-A9).
4. Boot path `_start → main → Engine_InitCore → Engine_InitSubsystems → Wad_InitLoader → VBlank
   wait → Boot_CreateServersAndEngineResources` and the main loop (C-B1…C-B6).
5. `g_ServerTable` is a 256-entry **server** table indexed by server id (C-C1); dispatch is
   `g_ServerTable[type_word & 0xffff]` (C-C2).
6. Full evidence chain for creating every server through the record pipeline and both
   factories; sizes, constructors and vtables of 25 server classes (C-C3…C-C5).
7. Master keeps its children sorted by `order_key` (descending) and updates them in that order
   every frame; flag bits 0/1 mark a child for removal (C-C6).
8. Pooled-server layout, init, bank selection and context stacks (C-C7, C-C8).
9. `WadTag_Object`, GroupEnd, ActivateByName and the streaming loader including payload
   ownership transfer via `param` bits 0x2000/0x4000 (C-D2…C-D4).
10. Texture creation path down to `Texture_ctor` (C-D5); GO server/bank/context functions
    (C-D6); material records are stashed, not built, at load time (C-D7).
11. ABI and code-generation facts (C-E1…C-E7).

## Main hypotheses (details in `hypotheses.md`)

* GCC 2.9x C++, no RTTI, -G0 (HIGH).
* There is no global object handle table (HIGH).
* GameObject contexts: `CXT_*` descriptor → `GoClassA4` context pushed by tag 5 → `go*`
  records built by the context (HIGH).
* Materials are constructed at GroupEnd (MEDIUM).

## Rejected / corrected

`g_ObjectHandleTable` → `g_ServerTable`; `Engine_RegisterServers` →
`Boot_CreateServersAndEngineResources`; `Wad_ProcessRecord` is the boot dispatcher, not the file
loader; tag 0x13 is not a section switch; `__CTOR_LIST__[0]` is 2, not −1; the Ghidra
truncation was caused by a non-volatile busy-wait, not a no-return flag. Full table:
`hypotheses.md` §Rejected.

## Unresolved questions

See `engine-reconstruction.md` §12. In short: ~~untraced tag handlers~~ (all traced 2026-10-02, C-A10), root vslots 12/13, who sets removal flags and the quit flag, where materials are
built, the 0x120 GO instance class, renMaster's frame, 96 unclassified Ghidra dead-code
removals, and runtime confirmation of all HIGH items.

## Artifacts produced in this phase

| Path | Content |
|---|---|
| `docs/confirmed.md`, `docs/hypotheses.md` | findings with confidence and validation |
| `docs/engine-reconstruction.md` | readable engine description, pseudocode |
| `docs/core-functions.md`, `analysis/core_functions.tsv` | 116 function records (callers, callees, vtable slots, globals, strings, evidence) |
| `analysis/core_callgraph.dot` | engine-core call graph: 99 direct calls + 41 verified virtual-call edges (dashed), coloured by confidence (`tools/core_graph.py`) |
| `analysis/core_decomp/*.c` | Ghidra output (pass 5, with recovered types) for confirmed/high functions |
| `analysis/disasm/*.s` | annotated disassembly of the key functions |
| `analysis/symbols.tsv`, `structs.txt`, `sigs.tsv`, `volatile.tsv` | the model applied to Ghidra |
| `analysis/vtables.tsv` | 231 vtables with installers and slots |
| `analysis/servers.tsv` | server classes: size, constructor, vtable chain, slots |
| `analysis/wad_object_paths.tsv` | per-server bank classes and creator functions |
| `analysis/xrefs_362c48.txt`, `xrefs_tag_handlers.txt`, `abi_evidence.txt` | raw scan output |
| `analysis/exports/pass{1..5}/` | function index, call graph, CFGs, decompilation per pass |
| `tools/eedis.py`, `eescan.py`, `vtables.py`, `server_profile.py`, `trace_paths.py`, `core_report.py`, `abi_evidence.py`, `decomp.py` | analysis tools |
| `tools/ghidra_scripts/ApplyCore.java`, `EnableParamId.java`, `ExportGoW2.java` | Ghidra automation |
| `gow2-rs/crates/gow2-model` | recovered tables/layouts with addresses and disc-backed tests (no engine code) |
| `snapshots/` | pass-1 artifacts, pass-2 Ghidra project |

## Phase 3 (started 2026-10-02): runtime validation, then outward to Kratos

`docs/project-goal.md` records the end goal: a Minecraft-based game where the player is Kratos with
the GoW II gameplay layer. It also sets the order of investigation after the engine core.

Done so far:
- PCSX2 2.8.2 is installed in `tools/pcsx2/` (portable, PINE on). See `runtime-validation.md`.
- Tools added: `tools/eeram.py` (PINE / savestate / raw RAM reader) and `tools/ramwalk.py`, which walks the server table, Master, pools and GO contexts/node trees with model checks.
- GOServer frame read statically: C-D8 (default bank slot 3), C-D9 (GoClassA4 slots 2/3, the node tree) and C-D10 (node layout, 0x120 instance). One new hypothesis: H-O6, "GO frame only sweeps; behaviour lives elsewhere".

- BIOS installed (SCPH-90001, dumped by the user).
- Capture `title` done: 60/60 checks OK. It confirms C-C1, C-C3…C-C8, C-D6, C-D9 and the H-O2 structure. It also corrected `servers.tsv` for renPrimMaster (R11).
- First Kratos lead: the `CXT_R_Hero` context holds node `gohero`. Details in `runtime-validation.md`.
- Hero position write-watch (idle and walking), full call stack:
  - Kratos is updated from **EvtServer_Update** → … → `Character_Update` (vtable 0x2f1440 slot 2) → `Node_SetMatrix`.
  - While walking, this is the only writer.
- Kratos stage started (`kratos-data.md`):
  - Character class hierarchy; player flag `+0x174 & 0x200000` (runtime-confirmed).
  - 13 controller types (`Player`/`hfsmPlayer`, `Enemy1`, …).
  - The hero gameplay data is the `DC_*` records (tags 0x0b–0x10) in `R_HERO*.WAD`: weapon attachments, per-weapon move banks, ~1,000 moves, 3,761 branches, and timed actions (sound, FX, camera shake, rumble, hit-stop, meter).

- **Pass "tag 0x10 decoding"** (`kratos-data.md` §6). DC record family:
  - The 0x0b container, 0x0c blob, 0x0d exports and 0x0e imports are confirmed from the handlers. 0x0f/0x10 are ignored debug tables (R12).
  - Hash function and lookup confirmed.
- **Move system decoded from code:**
  - `MOV` packed lists (branches / collisions / actions).
  - `tBranch` input, state and priority rules, with random tie-break and a ring-buffer input queue.
  - The action executor and its kind table.
  - Kratos's `tMoveSystem` located in RAM (`0x00957370`, current `MOV_Stand`).
- **Item ids ↔ data names code-confirmed:** 1 Lightning, 2 Electric, 3 Wind, 6 Earth, 0x10 Medusa (cast), 0x0e Bone, 0x0f Hammer, 0x11 Olympus (movesets).
- **Component systems:** generic vs player-only split from the `tMemoryPool` data.
- **Tools:** `tools/dcparse.py` and `tools/dcmoves.py`, output in `analysis/dc/R_HERO00/`, `analysis/dc/R_PERMA/`.

## Next investigation targets (in order)

-1. **Move system** (`kratos-data.md` §6.11):
   - branch dequeue → move change;
   - runtime watches on `moveSys+0x6c`, button bits and health;
   - `tCollision` → hits → damage;
   - action kind 0x0d family;
   - item modules.

0. **Kratos data path** (`kratos-data.md` §5):
   - the DC tag handlers 0x00120588…0x001208b8 and the layout of the tag 0x10 move/branch/action graph;
   - `Character_Update` input and move selection;
   - HFSM lookup;
   - weapon-bank switching.

1. **Runtime confirmation** (`runtime-validation.md`): `ramwalk.py report` at the title and
   in-game, plus the breakpoint table there.
2. **Who drives nodes** (H-O6):
   - `GOServer_Update` 0x00142b08: does it run all banks or only the default one?
   - `FUN_001435b8`: the 0xa0 attachment at node `+0x104`.
   - Which servers iterate node trees (AnimServer, BhvrServer, ScriptServer).
   - Trace one real `go*` record from WAD to node to whatever updates it.
3. **WadServer**: tag 0x15/0x16 handlers, WAD object layout (+0x38 components, +0x4c dict,
   +0x50 children, +0x9c heap, +0x1c0 parent, +0x1dc), and who calls `Wad_StreamUpdate`.
4. Material construction (MatServer context bank vslot 11).
