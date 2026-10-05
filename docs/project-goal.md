# Project goal and reverse-engineering priorities

_Recorded 2026-10-02. This document steers what gets reverse engineered in depth. It is not an
implementation plan: no mashup code is written until the relevant God of War II systems are
understood to the evidence standard in `confirmed.md` / `hypotheses.md`._

## Direction (set 2026-10-03, later; supersedes everything below)

**Rust rewrite on the original assets, Kratos first.** The user decided to rewrite the game in Rust and to
use the reverse engineering as the specification, instead of finishing the full decompilation first.

- **Order.** Kratos complete first: model and skeleton, animation, the full player controller, combat,
  magic, camera, sound, then HUD and UI. After that the rest of the game (enemies, levels), the broad
  decompilation, and the other-game mashup.
- **Assets.** All original assets are used. Development reads `extracted/` and `analysis/`; anything
  shipped reads the user's own ISO at runtime. `extracted/` and `analysis/exports/` are copyrighted and
  never go into a repository. Game values (for example `kratos_tuning.tsv`) load from game data, never
  hard-coded.
- **Engine.** Bevy to start (window, input, audio, rendering, loop). The core crates (formats, model,
  Kratos controller and combat) never import Bevy, so moving to plain `wgpu` later replaces only the front
  end. Gameplay runs on a fixed 59.94 Hz tick.
- **Reference.** `chasmlol/2010-rust-rewrite-mashup` (a Rust rewrite of a shipped game that loads the
  user's own install at runtime, several games as separate crates). Its engine was not verified.
- **Decompilation continues on demand.** The Rust code asks for the next unknown (input writer `+0x3b8`,
  collision probes, the Flash HUD movie `FLP_HUDA`); the decomp answers it. Python tools stay as the
  oracle: a Rust decoder must match the Python output on the same WAD.
- **Process.** The user playtests each milestone; a debug overlay and input record/replay support it.
  All work stays in one session; no sub-agents unless the user asks.
- **Ledger.** Track `rust` (rs-03 to rs-17) in the reconstruction ledger.

~~**Phase 1 only** (below): the earlier rule "no replacement-engine implementation, no Rust bias" is
lifted for the Rust port. The rule still holds for the recovered documentation: describe the original game
faithfully, do not shape the docs around Rust.~~ (kept: the faithful-documentation part still applies.)

### ~~Phase 1: complete reverse engineering (the only current goal)~~ (superseded)

The question is **"How does God of War II actually work?"**, not how to rebuild it in Rust or how to put Kratos into Minecraft.

Areas, all in scope:
- the executable, the engine and the data formats;
- game systems and gameplay;
- rendering, animation, audio, particles/effects;
- characters, AI, combat, weapons/magic;
- levels, scripting/events, UI/menus, save/load, cinematics/FMV;
- resource/memory management, libraries/interfaces;
- a classification of everything else discovered.

**Definition of done.** Every relevant game-owned subsystem/function is one of:
1. **understood**: documented well enough to reproduce its behaviour;
2. **library/external**: SDK, C runtime, middleware;
3. **dead**: unreachable/unused, with evidence;
4. **duplicate/thunk**;
5. **unresolved**: explicitly documented, with the reason and the remaining questions.

**Rules.**
- Follow the original game's architecture and behaviour as faithfully as the evidence allows.
- Do not choose or skip work because of any possible future implementation.
- Do not shape the recovered architecture around Rust.
- No replacement-engine implementation in Phase 1.

**Maintained throughout:**
- Ghidra analysis, recovered structures, function names/descriptions and call graphs;
- data format docs and runtime PCSX2/PINE evidence;
- confidence levels (CONFIRMED / HIGH / MEDIUM / LOW);
- scripts/decoders, coverage metrics (`tools/coverage.py`) and the ledger/history.

**Current priority:**
- broad decompilation across the executable;
- in parallel, the Kratos/combat chain: input → controller/HFSM → branch → MOV → animation → collision → damage → effects → state transition. It is one subsystem among many. Particles/rendering work stands for the same reason: it is part of the original game.

### Phase 2: decision point

Once Phase 1 is sufficiently complete, evaluate what was learned and decide what to build: a native Rust PC port, Minecraft integration, both, or another approach the reverse engineering suggests. None of these is assumed during Phase 1.

_The sections below record earlier goals and are kept for history._

## ~~Phases (set 2026-10-02)~~ (superseded 2026-10-03, see above)


~~1. **Complete decompilation of God of War II** (SCUS-974.81, NTSC-U v1.01): every function in the ELF and every on-disc format, to the CONFIRMED / HIGH / MEDIUM / LOW standard. Renderer, levels, scripting, audio, menus and FMV are all in scope now. Main metric: function coverage. Baseline: 6,405 functions (Ghidra pass5), 153 named, 2.4 %. `tools/coverage.py` reports it.~~
~~2. **Native PC port in Rust**: a runtime that loads the original disc data and plays the game. It builds on `gow2-rs` (readers) and the viewer work. No port code for a subsystem until that subsystem is decompiled.~~
~~3. **Minecraft integration**: the destination below, built on top of the working port instead of on isolated systems.~~

~~The original executable stays a reference: the port is our own implementation of the recovered behaviour.~~

## Destination (historical, 2026-10-02; superseded by Direction above)

A standalone Rust runtime that is **Minecraft at its core**, where the player is **Kratos** and
the player/combat layer comes from God of War II. Architectural inspiration:
[chasmlol/2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup)
(Minecraft + MW2 in one Rust runtime).

* Minecraft stays fully functional and provides:
  * the world: voxel terrain, chunks, caves, trees and day/night;
  * mining, block placement and crafting;
  * items, inventory and hotbar;
  * mobs (zombie, skeleton, creeper, spider, …), which stay the enemies.
* God of War II provides Kratos:
  * movement, third-person camera and combat;
  * combos, dodge, block/parry, hit detection, damage and hit reactions;
  * animation-driven gameplay events and feedback (effects, sound, camera shake);
  * the **full GoW II arsenal**: the weapons and the magic system.
* Both systems share one inventory. Example: pickaxe → mine → switch to the blades → fight a zombie with GoW combat → switch back. Mobs remain Minecraft entities, and their drops stay Minecraft items.

~~It is **not** "God of War II inside Minecraft", and it is not a full port of God of War II
levels.~~ Superseded 2026-10-02: a full PC port is now phase 2 (see Phases).

```
                 RUST RUNTIME
          ┌──────────┴──────────┐
   MINECRAFT SYSTEMS      GOD OF WAR II SYSTEMS
   world/blocks/chunks    Kratos: movement, camera
   items/inventory/hotbar combat, weapons, magic
   crafting, mobs         animation, collision, damage, effects
          └──────────┬──────────┘
                INTEGRATION
```

The Rust code is our own implementation of recovered behaviour. The original executable is a
reference, not a component.

## What must be recovered from God of War II

| Area | Needed |
|---|---|
| Player/Kratos | object architecture, state, init, per-frame update, movement (accel/decel, jump, ground detection, facing), movement states, world interaction |
| Camera | third-person follow, position/rotation, targeting, collision/obstruction, combat camera |
| Combat | attack states, combos, timing, hit detection, damage, hit reactions, cancels/transitions, dodge, block/parry, combat state |
| Weapons | for each weapon (Blades of Athena, Barbarian Hammer, Spear of Destiny, Blade of Olympus): internal representation, state, unlock, switching, light/heavy attacks, combo chains, hit volumes, damage, range/AoE, reactions, animation events, specials, resource cost, upgrade levels, effects/sound |
| Magic | representation, unlocks, meter, consumption/regeneration, upgrades, selection, input, casting states, animation, targeting, projectiles/areas, damage, reactions, collision, effects, sound, restrictions. The abilities themselves are to be identified from the game, not assumed |
| Animation | state machine, transitions, timing, animation-driven events |
| Collision/physics | player/enemy/attack/world collision, ground detection |
| GameObjects | lifecycle (create/destroy/update), components, parent/child, interaction |
| Feedback | hit/weapon effects, particles, sound triggers, camera feedback |

~~Lower priority, investigated only when the systems above depend on them: level streaming
beyond what loading Kratos needs, the renderer internals, FMV, menus and Flash UI.~~ Superseded 2026-10-02: all of these are in scope for phase 1.

## Order of work

_Original order for the gameplay systems. Under the phase plan it still orders the gameplay part of phase 1; the other subsystems are worked through by coverage._

```
engine-core runtime validation   (current)
  → GameObject system → object lifecycle
  → player/Kratos → player update/state → movement
  → camera → combat → weapons + magic
  → animation → collision/hit detection
  → enemy interaction → effects/audio/feedback
```

Every stage uses the same evidence standard:
* sources: disassembly, decompilation, xrefs, vtables, WAD/disc data, and PCSX2 runtime (breakpoints, memory watches, RAM dumps);
* confidence levels: CONFIRMED / HIGH / MEDIUM / LOW / SPECULATIVE.

Nothing is named or described because it "looks like" a conventional engine.

## Leads already relevant to the destination

* Hero data lives in the WADs. `HERO_HEAP_SIZE` is a tag-0 value record (`formats.md`). The
  hero WAD(s) and their `go*`/`ANM_`/`BHV_`/`COL_` records are the natural entry point for the
  player stage.
* Per-object behaviour is driven by GameObject contexts (`CXT_*` → `GoClassA4` → `go*`,
  `hypotheses.md` H-GO*). The GOServer frame (`0x00283510`, `0x00140ff8`) and the 0x120 instance
  class (`0x0013bf50`) are therefore the bridge from the engine core to Kratos.
* `AnimServer` (0x03), `BhvrServer` (0x14), `CollisionServer` (0x10), `CameraServer` (0x09),
  `EffectsServer` (0x19) and `SoundServer` (0x15) are the server-level owners of the systems
  listed above (`analysis/servers.tsv`).
