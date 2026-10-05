> **Superseded (2026-10-02).** This pass-1 overview contains claims that were later rejected (object handle table, `Engine_RegisterServers`). Current material: [engine-reconstruction.md](engine-reconstruction.md), [confirmed.md](confirmed.md), [hypotheses.md](hypotheses.md). Kept unchanged below for the record.

# God of War II engine — recovered architecture

Executable: `SCUS_974.81`, ELF32 MIPS R5900, entry `0x00100008`, single LOAD
segment at `0x00100000`. Stripped (no `.symtab`). 5,871 functions found by
Ghidra 12.1.4 + ghidra-emotionengine-reloaded.

| Section | Address | Size |
|---|---|---|
| .text | 0x00100000 | 0x1d7724 |
| .data | 0x002d7780 | 0x11780 |
| .rodata | 0x002e8f00 | 0x12ea8 |
| .bss | 0x002fbe00 | 0x836c8 |
| 23 × `.DVP.overlay..*` | — | VU0/VU1 microcode (renderer) |

Libraries identified from version strings: Sony `libipu`, `libgraph`, `libdma`,
`libcdvd`, `libdbc`, `libpad2`, `libmc2`, `libscf`, `libkernl` (all 3.0.x), IPU
MPEG shell `PsIIShlMpeg 1230`, and 989 Studios' **989snd** sound system.

## Language / ABI

- C++ built with an old GCC (2.9x-era). Virtual calls use 8-byte vtable entries
  `{ short this_delta; short index; void (*fn)(); }`, vtable pointer at object
  `+0x20`.
- Objects are reached through a u16 handle table `g_ObjectHandleTable`
  (`0x00362c48`): `obj = g_ObjectHandleTable[handle]`.

## Server tree (`Engine_RegisterServers`, 0x00189b10) — **high confidence**

The engine is a tree of subsystem "servers". At boot, each server is described
as a synthetic WAD object record (type word `(id << 16) | parent`) written into
a 2 KB buffer and sent through `Wad_ProcessRecord` (0x00189098) — the same path
level WADs take. Order and arguments as in the binary:

```
Master_Server (0x05)
├─ WadServer (0x16)        ├─ LightServer (0x06)
├─ ProServer (0x0a)        ├─ MatServer (0x08)
├─ GOServer (0x01)         ├─ TextureServer (0x07)
├─ AnimServer (0x03)       ├─ GfxClutServer (0x0c)
├─ BhvrServer (0x14)       ├─ SoundServer (0x15)
├─ EvtServer (0x13)        ├─ renMasterSvr (0x0d)
├─ CollisionServer (0x10)  │  ├─ renModelServer (0x0f)
├─ ScriptServer (0x04)     │  ├─ renParticleSvr (0x11)
├─ CameraServer (0x09)     │  ├─ renFlashServer (0x1b)
├─ WaypointServer (0x12)   │  ├─ renShadowServer (0x20)
├─ EffectsServer (0x19)    │  └─ renEEPrimSvr (0x17)
│                          ├─ renPrimMaster (0x02)
│                          └─ EpiServer (0x0b)
```

Each call also passes a priority-like value (Master 0x7fff … render servers
0x1000) and two pairs of sizes that look like pool capacities; their exact
roles are still open. Every one of the 277,483 object records in the disc's
292 WADs routes to one of these ids.

## Other confirmed functions

| Address | Name | Notes |
|---|---|---|
| 0x0018a0e0 | `Time_UpdateFrameRate` | RCNT0-based frame timing, clamps, ÷60 |
| 0x0018a328 | `GS_InstallIntcHandler` | `AddIntcHandler(INTC_GS)`, `GS_CSR = 6` |
| 0x0018a388 | `GS_RemoveIntcHandler` | |

The full curated list lives in `analysis/symbols.tsv`; it is applied to the
Ghidra database by `tools/run_ghidra.ps1`.

## Scripting / gameplay hints from strings (to be traced)

- Hundreds of `SCR_*` script callbacks (`SCR_MountPegasus`, `SCR_GrapplePoint`,
  `SCR_MedusaHeadBeam`, `SCR_TimedPress` …): native functions exposed to the
  level scripting layer.
- `DefaultHFSM`, `hfsmBalanceGeom`: hierarchical finite state machines drive
  behaviours.
- `PB_*` button prompts (`PB_CircleBtnSmash`, `PB_360CCW`): QTE minigames.
- Flash (`FSCommand:`, `GotoFrame`, `SetTarget`): the HUD/menus are Flash movies
  rendered by `renFlashServer`.
- Sound buses: Music, Mime Dialogue, Ambient Streams (U1) … Dry Effects (U12).
