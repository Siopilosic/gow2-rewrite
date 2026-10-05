# God of War II — reverse engineering

Target: **SCUS-97481** (NTSC-U, v1.01). Requires your own copy of the disc; no
game data is redistributed by this project.

Current phase: verification and semantic reconstruction of the engine core.
Start with [`docs/research-status.md`](docs/research-status.md).

## Documents

| File | Content |
|---|---|
| `docs/project-goal.md` | end goal (Minecraft + Kratos mashup) and the RE priorities it implies |
| `docs/research-status.md` | tool versions, pass history, summary, artifacts, next targets |
| `docs/kratos-data.md` | Kratos: character class, controller types, hero gameplay data (DC records) |
| `docs/runtime-validation.md` | PCSX2 setup, capture procedure, runtime checks and results |
| `docs/confirmed.md` | findings proven from the binary (and disc), with validation methods |
| `docs/hypotheses.md` | HIGH…SPECULATIVE interpretations, rejected claims |
| `docs/engine-reconstruction.md` | readable engine description with checked pseudocode |
| `docs/core-functions.md` | generated records for the 116 engine-core functions |
| `docs/formats.md` | disc and asset formats |
| `docs/environment-audit.md` | level inventory and progress of the 1:1 environment recreation |
| `docs/models.md` | MDL_ models, placement, materials and blend modes |
| `docs/rendering.md` | VU1 microcode, light records, fog post-process |
| `docs/animation.md` | ANM_ records: container, channel types, evaluator notes |
| `docs/particles.md` | PTC_ particle shapes, FXC_ emitters, VU1 program A op list, render routines, blend; viewer playback (`tools/ptc_export.py`, `analysis/levels/particles.js`) |
| `docs/collision.md` | Collision world (`DAT_002d7870`), query parameter block and hit record, segment/sphere queries, character move-with-collision `FUN_00226af0` |
| `docs/scripting.md` | ScriptServer, native script class registration (`SCR_*`), descriptor slots, script object layout; table from `tools/scr_registry.py` |
| `docs/combat.md` | Hit windows, hit detection, blocking, damage and knock-back; Kratos damage multiplier tables |
| `docs/magic.md` | Casting path (move actions start script natives), `Scr_Lightning` with stage data and button-mash charge |
| `docs/camera.md` | Camera manager, per-frame view build, camera zone selection by priority and blending |
| `docs/hud.md` | UI object and named variable table, HUD values (health, magic, God meter, orbs), game event queue |
| `docs/audio.md` | Sound actions per move, SBI/SBP sound records, 989snd SBlk bank, SBlk tables decoded, sounds exported by SND_ name at their pitch (`tools/sbi_export.py`, `tools/sbp_export.py`, `tools/move_sounds.py`) |
| `docs/effects.md` | Kratos's visual effects: move effects (`tools/move_fx.py` → `fx.tsv`), footstep surface dust and sound, blade fire and lights, blade and grapple trails |
| `docs/character-update.md` | `Character_Update` 0x00228f90: entry modes, locomotion state word `+0x170` (ground/air/rope/wall/water/Pegasus/ceiling/grapple), per-state handlers, Pegasus mount path, animation/death tail, callee table |
| `tools/coverage.py` | Phase-1 coverage: every game function in an end state (understood, library, dead, thunk, unresolved) or work in progress (partial, unknown); sources `analysis/classification.tsv` (explicit) and `analysis/triage.tsv` (automatic); `--regions` per 64 KB |
| `tools/triage.py` | Automatic triage into `analysis/triage.tsv`: trivial accessors, setters, empty stubs and one-call wrappers (thunk), and functions with no caller, data pointer or lui/addiu reference (dead) |
| `tools/scr_registry.py` | All literal `SCR_*` native registrations into `analysis/scr_registry.tsv` |
| `tools/missed_functions.py` | Function starts missed by Ghidra (address built by lui+addiu after `jr ra`) into `analysis/missed_functions.tsv`; created by `ApplyCore.java` |
| `tools/dataref.py` | Where an address is stored in data or built in code (vtables, registration tables, callbacks) |
| `tools/kratos_tuning.py` | Dumps Kratos's tuning block and speed profile from a RAM capture into `analysis/kratos_tuning.tsv`, with the documented use of each field |
| `tools/gltf_export.py` | Exports a WAD's models to glTF; rigged models get a skin. Characters: `--bind-space` (real inverse binds), `--anm ANM_x --clips a,b` (named clips). View with `analysis/levels/viewer.html?levels=<dir>&level=<name>&clip=<clip>` |
| `tools/gltf_attach.py` | Attaches a prop mesh from one glTF export to named joints of another (Kratos's blades on `lWeapIH`/`rWeapIH` → `analysis/levels/KRATOS_gltf`) |
| `docs/engine.md` | pass-1 overview (superseded, kept for the record) |

## Layout

| Path | What |
|---|---|
| `God of War II.iso` | your disc image (not part of the project) |
| `extracted/` | files pulled from the ISO (`SCUS_974.81`, IRX, TOC, `pak/*.WAD`) |
| `ghidra/GoW2_Analysis.gpr` | Ghidra project with the model applied (open in the GUI after installing the EE extension) |
| `analysis/symbols.tsv`, `structs.txt`, `sigs.tsv`, `volatile.tsv` | **the model**: names (with confidence/evidence), structures, signatures, volatile globals |
| `analysis/exports/passN/` | per-pass function index, call graph, CFGs and full decompilation |
| `analysis/core_decomp/`, `analysis/disasm/` | decompilation / annotated disassembly of the key functions |
| `analysis/vtables.tsv`, `servers.tsv`, `wad_object_paths.tsv`, `core_functions.tsv` | generated tables |
| `tools/` | Python analysis tools, Ghidra scripts, `run_ghidra.ps1` |
| `gow2-rs/` | Rust: disc/asset readers, recovered data model, analysis CLI (no engine code yet) |
| `snapshots/` | preserved earlier artifacts and Ghidra project |

## Workflow

1. Investigate in Ghidra or with `tools/eedis.py dis <addr>`, `tools/decomp.py <addr>`.
2. Record conclusions in `docs/confirmed.md` or `docs/hypotheses.md` with evidence.
3. Add names to `analysis/symbols.tsv` (only `confirmed`/`high` are applied as names;
   `medium`/`low` become comments), types to `structs.txt`/`sigs.tsv`.
4. Close the project in the Ghidra GUI and run
   `powershell tools\run_ghidra.ps1 -Pass <new-label>`; outputs go to
   `analysis/exports/<new-label>/` and earlier passes are never overwritten.
5. `GOW2_EXPORT=<label> python tools\core_report.py` regenerates the function records.

## Rust

```
cd gow2-rs
cargo test                                    # unit tests + disc tests (skipped without the ISO)
set GOW2_FULL=1 && cargo test --test disc     # sweep all 292 WADs
target\release\gow2 servers                   # recovered server registrations/classes
target\release\gow2 route ..\extracted\pak\R_PERMA.WAD
target\release\gow2 wad ..\extracted\pak\R_PERMA.WAD
target\release\gow2 textures ..\extracted\pak\R_SHELLA.WAD out\tex
```
