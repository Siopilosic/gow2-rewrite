# Hypotheses, open interpretations and rejected claims

Levels: **HIGH** (several independent pieces of binary evidence, no contradiction, but not
yet shown directly or at runtime), **MEDIUM** (one solid piece of evidence), **LOW**
(plausible), **SPECULATIVE** (an idea worth testing). Confirmed facts are in
[`confirmed.md`](confirmed.md). An entry moves there only when the binary shows it directly.

Every entry says what would confirm or reject it.

---

## Toolchain and ABI

### H-T1 The game was compiled with a GCC 2.9x-era C++ compiler — HIGH
Evidence: GCC 2.x non-thunk vtables (C-E1), `__in_chrg`-style destructors (C-E3), libgcc2
`__main`/`__do_global_ctors` (C-E5), a static initialiser called with `(1, 0xffff)` which matches
`__static_initialization_and_destruction_0(int, int)` (C-E5), and MIPS GCC divide-by-zero
traps (C-E7). There are no compiler strings. SN Systems ProDG (EE GCC 2.95 based) is common
on PS2 but nothing in the binary names it.
Confirm: match libgcc2/libstdc++ functions byte-for-byte against an ee-gcc 2.95/2.96 build.

### H-T2 Built without RTTI and with small data disabled — HIGH
Vtable entry 0 is all zeros in all 231 tables (with RTTI it would point to type info); no
`$gp`-relative accesses (C-E7).
Confirm: build a test class with ee-gcc 2.95 `-fno-rtti -G0` and compare the vtable layout.

### H-T3 Several server methods are duplicated code (templates or copy/paste) — MEDIUM
`TextureServer_Init` and `GOServer_Init`; `Mgr_Create` and `Bank_Create`;
`Root_CreateFromRecord` and `Mgr_CreateFromRecord`; the three `*_SelectBank`;
`BootWad_DispatchRecord` and `Wad_DispatchRecord` have identical instruction streams.
Confirm: byte-compare all functions and group identical bodies (template instantiations
usually also share call targets).

---

## Boot and main loop

### H-B1 `0x002d7fdc` is the request-quit flag — HIGH
It is the only loop condition of `Engine_MainLoop`. Who sets it is unknown.
Confirm: find writers of 0x002d7fdc.

### H-B2 `Engine_Shutdown` (0x00187508) tears the engine down — MEDIUM
It runs after the main loop and calls four functions that have not been traced.

### H-B3 Cause 2 is VBlank start; `g_VBlankCount` counts frames — HIGH
The handler registration (cause 2) and the counter are confirmed (C-B6). That cause 2 means
VBlank start comes from PS2 kernel documentation, not from the binary.
Confirm at runtime: the counter increases by 60/s on NTSC.

### H-B4 `FUN_001400c8(name, fn)` registers native script callbacks — ~~MEDIUM~~ HIGH, resolved 2026-10-03 (`docs/scripting.md` §2)
`FUN_0025a1f0` calls it 13 times with `"SCR_Sky"`, `"SCR_Weather"`, … and a function pointer
loaded from a table.
Confirm: trace `FUN_001400c8` into the ScriptServer.

### H-B5 `Time_UpdateFrameRate` computes the frame-time scale — MEDIUM
It reads RCNT0, clamps 100..400, divides by 60.

---

## Servers

### H-S1 `order_key` is the server update priority — HIGH
Confirmed: Master's children are sorted by it (descending) and updated in that order
(C-C6). Not shown: whether other code uses it (it is also `+0x10` of non-server objects).
Confirm: runtime breakpoint on `Mgr_UpdateChild` and record the order of `a1`.

### H-S2 `arg_a3` / `arg_t0` of server records are pool sizes — MEDIUM (pooled servers HIGH)
In the pooled constructors `arg_t0` (header +0x14) is the capacity of each bucket and `arg_a3`
(+0x18) sizes the u32 array at +0x34 (C-C7). For non-pooled servers (Master, EvtServer,
ProServer, EpiServer, renMaster) their use is unknown.

### H-S3 `PooledServer` +0x34/+0x38/+0x3c form a free-index queue — MEDIUM
`ServerInit` takes `idx = field_34[field_38++]` when `field_38 != field_3c`, otherwise
appends to the bucket. The code that frees indices has not been read.

### H-S4 Context-stack capacity — LOW
The stack starts at +0x48 and the top index is at +0xc8, so the layout leaves room for 32
entries; no bounds check was seen.

### H-S5 renPrimMaster (id 2) is a different kind of class — MEDIUM
Its constructor 0x0016d740 installs four vtables, the last one with 6 slots; the factory gives
0x16c bytes. Not traced.

### H-S6 Root object slots 12/13 and fields +0x30/+0x34 — LOW
`Engine_UpdateServers` calls root vslot 12 or 13 when +0x30/+0x34 are non-zero. Meaning
unknown.

### H-S7 `SeqId_Assign` table — LOW
`g_SeqIds[n] = counter++`, called by 12 constructors (GO with 1, Camera with 9, …). It may
order per-class work; unknown.

---

## Objects

### H-O1 There is no global object handle table — HIGH
Objects are referenced by pointer; the owning server is found from `type_word & 0xffff`
(C-C2); names resolve through dictionaries (`Obj_FindByName`). Inside each pooled server
objects have an index (`pool_index` +0x1c for GO objects, `field_06` for banks) into that
server's bucket array. No generation counters were seen.
Confirm: show that no function maps an integer handle to an object outside the per-server
pools.

_Refinement (2026-10-02, `kratos-data.md` §6.8)._ The **game layer** does use handles: table
`0x002d8924`, 12-byte entries, index = `handle >> 23`, valid when `handle & 7 == 0` and the
entry's first word equals the handle (`FUN_0024deb0`, `FUN_0024eb00`). H-O1 still stands for the
engine core (servers/WAD objects).

### H-O2 GameObject context architecture — HIGH (structure runtime-confirmed at the title screen)
_Runtime (`runtime-validation.md`, capture `title`): 7 `GoClassA4` contexts in the GO default bank,
each with its 0x54 `CXT_*` descriptor at +0x7c, and descriptor +0x18 pointing back to the context.
Not yet observed: the push sequence during loading (needs the breakpoint below)._

Every step is confirmed individually (C-D2, C-D3, C-D6, C-C8). The full runtime sequence is
inferred:
1. A header record `CXT_<name>` (type 0x80000001, instance) reaches GOServer's default bank,
   which builds a 0x54-byte descriptor (`GOBank_NewDescriptor`). It is registered by name
   because its owner is 1.
2. A tag-5 record `CXT_<name>` (`WadTag_ActivateByName`) finds the descriptor and calls
   `GOServer->vslot5(desc)` (`PooledServer_Create`) → default bank `Bank_Create` →
   `GOBank_NewObject` builds a 0xa4 `GoClassA4` object from the descriptor, then
   `GOBank_OnCreated` registers it in GOServer's pool. Back in `WadTag_ActivateByName`,
   `owner(ctx)->vslot16(ctx)` = `GOServer_PushContext(ctx)` makes it current.
3. Ordinary `go*` records then select that context with `GOServer_SelectBank` and are built by
   `GoClassA4_CreateFromRecord`.
Confirm at runtime: breakpoint `GOServer_PushContext`; `a1` must be the object returned by
`GOBank_NewObject` for `CXT_R_Perm`/`CXT_ScreenGO`.

### H-O3 Runtime game-object instances are the 0x120-byte pool elements — MEDIUM
`GoClassA4_NewInstance` allocates them from the context's 0x120 pool and
`GoClassA4_FreeInstance` releases them. When they are spawned has not been traced.

### H-O4 The 0x70-byte GO node holds a transform — LOW
It copies 0x48 bytes from `data+0x1c` (a 4×4 float matrix + 8 bytes would fit).
Confirm: decode the floats of real `go*` records and check for orthonormal rotations.

### H-O5 GO subtype-1 records (0x40-byte object) describe a skeleton — SPECULATIVE
16-byte entries initialised to 0xffff/0xcf000000 and name hashes (`FUN_00181428`).

### H-O6 The GOServer frame only sweeps; GameObject behaviour is driven elsewhere — MEDIUM
Evidence:
- The per-frame path is `GOServer_Update` → default bank slot 3 (C-D8) → `ctx->vslot3()`.
- For the class that owns `FUN_00283410` (vtables 0x002f6610/0x002f66f0), `ctx->vslot3()` is `FUN_00140ff8`. That function only removes flagged nodes (C-D9).
- Nodes have no vtable (C-D10), so a generic `node->update()` call is impossible.

Behaviour (animation, scripts, "bhvr") is therefore expected to be driven by other servers
operating on nodes or on the `+0x104` attachment. The 0x0a0 object is built by `FUN_001435b8`.

Open:
- The list may also contain context classes with a different slot 3.
- `GOServer_Update` may update banks other than the default one.

Confirm: at runtime, list the vptrs of every context in the default-bank list
(`tools/ramwalk.py … go`), and break on `FUN_00140ff8` to see it runs once per active context
per frame.
Relevance to `project-goal.md`: the player (Kratos) is expected to be a node plus attachments,
so the code that moves it must be found via the servers that touch nodes. Candidates are
AnimServer 0x03, BhvrServer 0x14 and ScriptServer 0x04.

---

## WAD loading

### H-W1 Materials are built at GroupEnd — MEDIUM
`MatBank_CreateFromRecord` only stashes the payload (C-D7). Material records are group parents
(followed by their ANM/TXR references), so the real construction probably happens in the
material server's vslot 11 at GroupEnd.
Confirm: read MatServer's context-bank vslot 11.

### H-W2 The context bank used at runtime is the class installed by default-bank vslot 19 — HIGH (GO), MEDIUM (others)
For GO this is shown by H-O2. For the other servers, `tools/trace_paths.py` assumes the same
pattern (`analysis/wad_object_paths.tsv`). The texture class 0x002f7250 has the expected
`TextureBank_NewTexture`.

### H-W3 `param` bits 0x1000 and 0x2000 — MEDIUM
0x2000 is set by the loader when the payload was copied to the heap (confirmed); 0x4000 means
"buffer kept by the object" (confirmed from setter and checker). 0x1000 is set together with
0x4000 by model/animation and alone by sound; meaning unknown.

### H-W4 Sound records are filtered by a language/region mask — SPECULATIVE
The sound bank creator returns 0 when bits 4..11 of the subtype don't match
`*(0x002d79c0 + 4 * *0x002d7a44)`.

### H-W5 `Wad_StreamUpdate` budget — MEDIUM
It loops until a counter (Ghidra label `Count`, likely an EE timer register) passes
start + 0x781ec.

### H-W6 Animation objects are relocated in place — LOW
The animation bank returns the (copied or adopted) payload itself as the object.

### H-W7 Disc-name labels for tags 0x13/0x15/0x16 — LOW
The pass-1 names `HeaderEnd`, `WadHeader`, `PopHeap` come from record names on the disc.
Tag 0x13 shares its handler with tag 5 (activate by name); 0x15 and 0x16 are untraced.
**Resolved 2026-10-02 (C-A10):** 0x15 opens a WAD scope (heap + dictionary), and 0x16 (`PopHeap`) closes it.

---

## Rejected or corrected (from pass 1 and during this phase)

| # | Earlier claim | Status | Evidence |
|---|---|---|---|
| R1 | `0x00362c48` is a "u16 handle → object*" table (`g_ObjectHandleTable`) | **Rejected** | Indexed by server id; 256 entries; only servers write it (C-C1, C-C4). Renamed `g_ServerTable`. |
| R2 | `0x00189b10` "Engine_RegisterServers" only registers servers | **Corrected** | It also builds five engine banks and two materials (C-C3). Renamed `Boot_CreateServersAndEngineResources`. |
| R3 | `0x00189098` "Wad_ProcessRecord" is the WAD loader | **Corrected** | Its only caller is the boot path; files go through `Wad_StreamUpdate` → `Wad_DispatchRecord` 0x0018d6d8, an identical copy (C-D4). |
| R4 | Server args are "pool_a/pool_b/priority/cap_a/cap_b" | **Corrected** | `t1` is the sort key (C-C6); `t0` is bucket capacity for pooled servers; others unknown (H-S2). |
| R5 | `__CTOR_LIST__` starts with −1 | **Rejected** | It starts with 2; the −1 case exists only in the code (C-E5). |
| R6 | Ghidra truncated `FUN_0025a1f0` because `Wad_InitLoader` is marked non-returning | **Rejected** | No function was marked non-returning; the cause was the non-volatile VBlank busy-wait (C-F1). |
| R7 | Tag 0x13 switches the WAD from a header section to a data section | **Rejected** | It shares the handler with tag 5 (C-A5, C-D3); nothing switches sections. |
| R8 | "Objects are reached through a handle table" (pass-1 `engine.md`) | **Rejected** | See R1 and H-O1. |
| R9 | Pass-1 Rust `Engine::tick` = engine behaviour | **Removed** | It was a stub; the crate is now the analysis-only `gow2-model`. |
| R10 | `WadTag_Object` bounds-checks group pushes | **Rejected** | No check on push; only linking requires `top < 0x10`. |
| R14 | MDL_ 88-byte record `+0x50` = 1 marks a sky dome, drawn camera-relative (`models.md`, MEDIUM; RHOD10 only) | **Withdrawn** | Survey of 7,817 records in 122 level WADs: values 1/2/3/4/0x14. Value 1 is also on ropes, mirrors, lens flares, `ao*` actors and `group4217`; several sky domes carry 2. The meaning is UNKNOWN. Exporter now keeps it raw (`extras.mdl50`). The viewer picks skies by name (heuristic) and keeps them in world space by default. |
| R15 | MDL_ 88-byte record `+0x48` multiplies positions: `world = scale · v/16 + offset` (`models.md`, HIGH) | **Corrected** | It divides: rig root joints (30+ in RHOD10) carry translation = `+0x38` and rotation rows = 1/scale. Model extents are plausible only with the division (bridgeA 560 vs 8,954 units). Exporter default is now `--model-scale div`. |
| R13 | GFX pixel data is linear for 4 and 8 bpp, encodings 0 and 2 (`formats.md`, pass 1) | **Corrected** | 8-bpp with encoding 0 is PSMT8-swizzled; encoding 2 is linear. Visual evidence in `formats.md`. The pass-1 check had not covered an enc-0 8-bpp texture with recognisable content. |
| R12 | WAD tag 0x10 (`DC_*`) is the combat/move data (`kratos-data.md` first pass) | **Corrected** | The tag 0x0f and 0x10 handlers (0x00120898, 0x001208b8) parse nothing. 0x10 is a debug object index `{blob_off, name_off, type}`. The move data is the tag 0x0c blob with export/import tables 0x0d/0x0e (`kratos-data.md` §6.1). |
| R11 | renPrimMaster (id 2) final vtable is 0x2f44e8 (`servers.tsv` v1) | **Corrected by runtime** | Its vptr in RAM is 0x2f3688. 0x2f4520/0x2f44e8 belong to helper objects (vptr at +0x60) allocated inside the ctor. `server_profile.py` fixed (`runtime-validation.md`, capture `title`). |
