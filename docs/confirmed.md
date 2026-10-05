# Confirmed findings

Target: `SCUS_974.81` (SHA-256 `FD403603…F7DD6`), God of War II NTSC-U v1.01.

An item is listed here only when the binary itself shows it: the instructions were read
(usually both in Ghidra and with the independent disassembler `tools/eedis.py`), and where a
disc format is involved the disc data agrees. Interpretations that go beyond what the code
shows are in [`hypotheses.md`](hypotheses.md), even when likely.

Each entry gives **Evidence** (addresses; disassembly listings are in `analysis/disasm/`,
decompiler output in `analysis/core_decomp/`) and **Validation** (how to re-check it,
statically and at runtime). Runtime checks use the PCSX2 debugger with the stated breakpoints
or memory watches. None of the runtime checks have been run yet; everything here is static.

---

## A. Disc and file formats

### C-A1 Disc layout
Two ISO9660 volumes. Layer 0 (blocks 2,083,888) holds `SYSTEM.CNF`, `SCUS_974.81`,
`IOPRP300.IMG`, 10 `.IRX`, `GODOFWAR.TOC`, `PART1.PAK`. Layer 1, base sector 2,083,872, holds
`PART2.PAK`. `SYSTEM.CNF`: `BOOT2 = cdrom0:\SCUS_974.81;1`, `VER = 1.01`.
- Evidence: primary volume descriptors at sectors 16 and 2,083,888.
- Validation: `cargo test --test disc toc_has_913_files_and_two_layers`.

### C-A2 TOC/PAK
`u32 count; {char name[24]; u32 size; u32 copies; u32 first}[count]; u32 sector[]`. Sector
values ≥ 10,000,000 address `PART2.PAK` at value−10,000,000. 913 files, 1,532 copy sectors,
all copies of a file byte-identical.
- Evidence: disc data (every entry read; all copies compared, `tools/gowtoc.py`).
- Validation: `cargo test --test disc duplicate_copies_are_identical`.
- Not yet checked against the executable's own TOC reader (see hypotheses H-F1).

### C-A3 WAD record header and stride
`struct { u16 tag; u16 param; u32 size; char name[24]; }` followed by `size` payload bytes;
records are 16-byte aligned.
- Evidence (executable): `BootWad_BeginRecord` 0x00189160 stores `sh tag,0(buf)`,
  `sh param,2(buf)`, `sw 0,4(buf)`, then 24 name bytes at +8 (`BootWad_WriteName` 0x001890f8).
  `Wad_DispatchRecord` 0x0018d6d8 returns `((hdr->size + 15) & ~15) + 0x20`.
- Evidence (disc): all 292 standard WADs parse exactly to EOF; 277,483 object records.
- Validation: `GOW2_FULL=1 cargo test --test disc every_wad_parses_and_layouts_hold`.

### C-A4 Tag 0 records carry no payload
- Evidence: `Wad_StreamUpdate` 0x0018d10c `lhu a1, tag(hdr)`; `beqz a1 → 0x0018d130
  jal Wad_DispatchRecordNoPayload` without reading payload bytes; the same branch is taken when
  the aligned size is 0.
- Validation: breakpoint 0x0018d130 while a level loads; `*(u16*)0x0032f758` is 0 or the
  record size is 0.

### C-A5 Tag handler table
`Wad_InitLoader` 0x0018cc00 zeroes 0x17 entries of `g_WadTagHandlers` (0x0036a6b8) and
registers, via `Wad_SetTagHandler(tag, fn)` 0x0018d988:
`0→0x0018d7c0, 1→0x00185748, 2→0x00185968, 3→0x00185978, 4→0x0018ce20, 5→0x001859f0,
6→0x00185a78, 0x13→0x001859f0, 0x14→0x00185a78, 7→0x0018d7f8, 8→0x0018d870, 9→0x0018d8f0,
0x15→0x0018cb68, 0x16→0x0018cbe0, 0x11→0x00159c78, 0x0b..0x10→0x00120588/5e8/698/7b8/898/8b8`.
Tags 0x0a and 0x12 have no handler; the dispatcher skips the call when the entry is null
(`beqz v0` at 0x001890b8 / 0x0018d6f8) but still returns the stride.
- Validation: dump 0x0036a6b8..0x0036a714 after boot and compare.

### C-A6 GFX/PAL images
`{u32 type=0x0C; u32 w; u32 h; u32 encoding; u32 bpp; u32 count;}` + data; 256-colour CLUTs in
GS CSM1 order; PS2 alpha (0x80 = opaque).
- Evidence: decoding 221 images from two WADs gives correct pictures (checked visually).
  This is empirical; the executable's GFX reader has not been traced (H-F2).

### C-A7 TXR payload (88 bytes)
`u32 type=7; char gfx[24] @4; char pal[24] @0x1c; char ref3[24] @0x34; u32 @0x4c; u32 @0x50;
u32 @0x54`.
- Evidence (executable): `Texture_ctor` 0x001745d8 looks up the strings at +4, +0x1c, +0x34 in the
  current dictionary and copies the u32s at +0x4c/+0x50/+0x54.
- Evidence (disc): all 9,200 `TXR_` records in the first 120 WADs are exactly 88 bytes, with
  `GFX_*`/`PAL_*` names at those offsets.
- Validation: `cargo test --test disc` (every TXR record must parse as 88 bytes).

### C-A8 Server-instance record payload (48 bytes)
`{u32 type (bit31|server); u32 0; u32 a; u32 b; u32 c; u32 d; char name[24]}`.
- Evidence (executable): `BootWad_BeginInstance` 0x00189870 writes type `(0x8000<<16)|id`,
  then `0, a2, a3, t0, t1`, then a 24-byte name.
- Evidence (disc): every `*X_R_Perm`/`*X_ScreenGO` record is 48 bytes with that shape, e.g.
  `GFXX_R_Perm = [0x8000000C, 0, 0x400, 0x400, 0, 0]`. The engine's own `GFXX_engine` record
  uses the same 0x400/0x400 (`Boot_CreateEngineBank` 0x001899e8).

### C-A9 GroupStart/GroupEnd semantics, checked on the whole disc
GroupStart only sets `g_WadGroupPending`; the next Object becomes the group parent.
- Evidence: `WadTag_GroupStart` 0x00185968 is `sw 1, 0x00366008`; `WadTag_Object`
  0x00185914..0x00185940 consumes the flag and pushes the object.
- Disc: all 93,041 GroupStart records are immediately followed by an Object record; a
  GroupStart never occurs while one is pending; maximum nesting is 3.
- Validation: `cargo test --test disc_model`; `gow2 route <wad>`.

### C-A10 The remaining tag handlers (0, 6/0x14, 7, 8, 9, 0x15, 0x16), 2026-10-02
All read from pass5 decompilation. With C-A5…C-A9, C-D2/C-D3 (0x0b–0x11) and tags 1–5/0x13, **every registered WAD tag handler is now traced**. Tags 0x0a and 0x12 have none.

**Helpers used below:**
- `Dict_GetCurrent` 0x001816c8: top of the dictionary stack `*0x32f280[*0x32f284]`.
- `Dict_Insert` 0x00181860 (hash name, insert).
- Dict stack push 0x00181748 / pop 0x00181770.
- Heap stack push 0x0014b778 / pop 0x0014b7a0 / top 0x0014b6b0 (stack `*0x2fd330[*0x2fd334]`).
- `Heap_Alloc(heap, size, align)` 0x0014b328.
- `memcpy` 0x002c6acc.

**The handlers:**

| Tag | Handler | Behaviour | Disc (292 WADs) |
|---|---|---|---|
| 0 | 0x0018d7c0 | **Named integer.** `Dict_Insert(cur, hdr.name, hdr.size)`, then sets `hdr.size = 0`, so the stride is header only (explains C-A4). | 129: `EntityCount` ×123 (e.g. ATLAS210 = 42), `HERO_HEAP_SIZE`, `UPGRADE_HEAP_SIZE`, `N_SCREENS`, `MSGS_COUNT`, `MSGS_LINES`, `DELAY_00` |
| 6, 0x14 | 0x00185a78 | **PopContext.** `g_ServerTable[data[0]]->vslot17()` (pop; vtable `+0x88/+0x8c`). It mirrors tag 5/0x13, which pushes (C-D context push/pop, vslot 16/17/18). | 1 (`" PopContext"`, server 1); 0x14 unused |
| 7 | 0x0018d7f8 | **Named string.** `p = Mem_New(size+1)`, copy, NUL-terminate, `Dict_Insert(name, p)`. | 2: R_PERMA `MC_DATA` (memory-card header `BASCUS-97481GOWII`), `MC_ICONSYS` (`PS2D` icon.sys) |
| 8 | 0x0018d870 | **Named length-prefixed text.** `p = Mem_New(size+5)`, `*p = size`, copy at `p+4`, NUL, insert. | 1: R_PERMA `MSGS_TXT` (50,569 B, `*n*` message table) |
| 9 | 0x0018d8f0 | **Named raw blob, first wins.** Only if `Obj_FindByName(name)` is null: `Heap_Alloc(top heap, size, 16)`, copy, insert. | 1,567: `NCV_*` curves (rails/splines), `MSH_*` shapes |
| 0x15 | 0x0018cb68 | **Open WAD scope.** Sets `*0x2d8170 = 1` (loading). `scope = FUN_0018d9b8(data[0], data[1], hdr.name)`, then pushes `scope` on the WAD stack `0x32f778[++*0x32f7b8]`, pushes `scope[3]` (heap) on the heap stack and `scope[1]` (new empty dict) on the dict stack. | 292: first record of every WAD, `WAD_<name>`. `data[0]` = heap size: 8 MiB ×123 (levels), 1 MiB ×104, others. `-1` = no own heap. |
| 0x16 | 0x0018cbe0 | **Close WAD scope (`PopHeap`).** `*0x2d8170 = 0`; pop WAD stack, heap stack and dict stack; return the scope object to the WadServer pool `+0xd8`. The heap itself is **not** freed here. | 292: last record of every WAD |

**`FUN_0018d9b8` (WAD scope create).**
- The scope object comes from the WadServer pool `*(WadServer+0xd8)`:
  - `[0]` = an 8-byte object (`FUN_001208d8` → `FUN_00281038`);
  - `[1]` = an empty dictionary (`Mem_New(8)`, `+4 = 0`);
  - `[2]` = heap block;
  - `[3]` = heap handle.
- The block is carved from the parent heap `*0x2d8164`:
  - `Heap_Alloc`, or `FUN_0014b508` when `*0x2d8160 != 0` (alternate end?);
  - the request is clamped to the largest free block (`FUN_0014b828`);
  - the heap is created over it by `FUN_0014ab80(block, size, name)`, the same constructor as the root heap (C-B2).
- `*0x2d8158`, when non-zero, overrides the requested size once.
- It also allocates a new 0x80×0xc pool at `WadServer+0xdc`.

**Consequence.** Each WAD loads into its own heap and its own name dictionary, opened by its first record and closed by its last. Whether `Obj_FindByName` searches the whole dict stack or only the top is not read yet. H-W7 is resolved: the disc labels `WadHeader`/`PopHeap` match open/close of a scope.

**Validation:** break on 0x0018cb68/0x0018cbe0 during a level load, then compare `*0x32f7b8` and the dict stack depth before and after.

---

## B. Boot sequence and main loop

### C-B1 `_start` (0x00100008)
Zeroes 128-bit registers, clears `.bss` 0x002fbe00–0x0037f4c8 with `sq`, syscall 0x3c
(SetupThread: gp=0x00303df0, stack 0x01ff8000-0x8000…) and 0x3d (SetupHeap), calls
`FUN_002d5008`, `FlushCache(0)`, `Mem_InitRootHeap`, `ei`, then `main(*0x002fbe00, 0x002fbe04)`,
then `j 0x002d5218` with main's return value.

### C-B2 `Mem_InitRootHeap` (0x00146318)
Calls `FUN_0014ab80(align16(0x0037f4c8), 0x01ff7ff8 - start, "Root", 0, 0)`, stores the heap at
0x002d7a58, writes `0x0BADC0DE` at 0x01ff7ffc.

### C-B3 `main` (0x001463b8) call order
`__main; FUN_001875e0(argc, argv, *0x00337018); FUN_00187788; FUN_00146178; FUN_0018ae28;
Engine_InitCore; FUN_001886b0; Engine_InitSubsystems; Engine_MainLoop; Engine_Shutdown;
return 0`. argc/argv saved to 0x0036385c/0x00363860.

### C-B4 Initialisation chain to the server bootstrap
`Engine_InitCore` 0x00187468 → `FUN_0014d2c0`, `ServerTable_Init`, `FUN_0018a490(0x0032f2d0)`.
`FUN_0018a490` → `VBlank_InstallHandler` 0x0018a408 (`AddIntcHandler(2, 0x0018a3c8, 0)`) and
`GS_InstallIntcHandler` 0x0018a328 (`AddIntcHandler(0, 0x0018a278, 0)`).
`Engine_InitSubsystems` 0x00187498 → … → `FUN_0025a1f0` → `Wad_InitLoader`; waits for
`g_VBlankCount` (0x0036a6a0) to change 60 times, calling `FUN_00188e08` each time; →
`FUN_0018a580` → `Boot_CreateServersAndEngineResources` (0x0025a2c0) → … → 13 ×
`FUN_001400c8("SCR_…", fn)`.
- Validation: breakpoints on 0x00186f18, 0x00189b10 and 0x00187570 hit in that order.

### C-B5 Main loop
`Engine_MainLoop` 0x00187570: `while (*(u32*)0x002d7fdc == 0) Engine_Frame();`.
`Engine_Frame` 0x00187540: `Engine_UpdateServers(); FUN_0025ed60(); Time_UpdateFrameRate();`.
`Engine_UpdateServers` 0x00187388: `root = g_ServerTable[0]; if (root+0x30) root->vslot12();
else if (root+0x34) root->vslot13(); g_ServerTable[5]->vslot3();`.

### C-B6 VBlank counter
`0x0018a3c8` increments 0x0036a6a0 and 0x002d8108, xors 0x002d810c with 1, ends with
`sync; ei; jr ra; v0=0`. It is registered as the handler for INTC cause 2. Nothing else
writes 0x0036a6a0 except a reset to 0 in `FUN_0018a490`.

---

## C. Server table and server architecture

### C-C1 `g_ServerTable` (0x00362c48): 256 pointers indexed by server id
- `ServerTable_Init` 0x00186f18 zeroes entries 0..0xff, then zeroes 0x00363048.
- Entry 0 = a 0x38-byte root object (vptr 0x002f6a48 → 0x002f6968) built inline.
- All other entries are written by the objects themselves (C-C4). The only direct store with
  displacement 0x2c48 is entry 0 (raw encoding scan, `tools/eescan.py imm 2c48`).
- 884 references to the table; constant-offset ones are `+4*id` for registered ids (0x58 →
  0x16 ×173, 0x04 → 1 ×95, 0x4c → 0x13 ×84, …).
- Validation: after boot, entries are non-null exactly at {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa,
  0xb, 0xc, 0xd, 0xf, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x19, 0x1b, 0x20} and
  `*(entry) & 0x0fff0000` equals `id << 16` for each.

### C-C2 Dispatch rule: `owner = g_ServerTable[obj->type_word & 0xffff]`
Every virtual call on an engine object goes through the server named by the low 16 bits of
its first word, passing the object as an explicit argument. No null check on the table entry
(`WadTag_Object` 0x001857c8..0x001857f4: `lw v1,(table+4*id)` then `lw v0,0x20(v1)`).

### C-C3 Server records are boot WAD records
`Boot_CreateServersAndEngineResources` 0x00189b10 (previously named `Engine_RegisterServers`):
1. 25 records via `BootWad_AddServer4/6(name, parent, id, a3, t0, t1[, t2, t3])`. Each writes a
   tag-1 record, type word `(id << 16) | parent`, payload `{type, 0, a3, t0, t1[, t2, t3]}`
   (0x00189640, 0x00189728, 0x00189200), and dispatches it through `BootWad_DispatchRecord`
   (0x00189098), which is the same algorithm as `Wad_DispatchRecord`. Arguments: see
   `gow2-rs/crates/gow2-model/src/servers.rs` (`REGISTRATIONS`).
2. `Boot_CreateEngineBank(name, id)` 0x001899e8 for `GFXX/TXRX/MATX/LGTX/FLPX` (ids 0xc, 7, 8,
   6, 0x1b): builds a 48-byte instance record `"<name>_engine"` and calls the owner's vslot
   10, 5, 6 and 16 directly.
3. Two built-in materials `"stencil"` and `"trail1"` through `FUN_001892d0`/`FUN_001894a8`
   wrapped in GroupStart/GroupEnd records (`BootWad_EmitEmpty`).
- Stack frame 0x860 bytes; 2,048 bytes of it are the record buffer `g_BootWadBuf`. Saves
  s0–s2 (`sq`), ra (`sd`), f20/f21. No arguments, no return value. Writes `g_BootWadBuf`,
  `g_BootWadPos`, `g_BootWadSize` and clears `g_BootWadBuf` on exit. One caller (0x0025a2c0).

### C-C4 Evidence chain for every server
```
Boot_CreateServersAndEngineResources                       0x00189b10
 → BootWad_AddServer6(name, parent, id, …)                 0x00189728
 → BootWad_BeginObject: type = (id<<16)|parent              0x00189200
 → BootWad_EndAndProcess → BootWad_DispatchRecord           0x00189278 → 0x00189098
 → g_WadTagHandlers[1] = WadTag_Object                       0x00185748
 → g_ServerTable[parent]->vslot10(hdr, data)                 Master: Mgr_CreateFromRecord 0x0027f9f0
 → this->vslot5(data) = Mgr_Create                           0x00273290
     → this->vslot26() (existing?) / this->vslot19(data)    Master_NewServer 0x00185be0
                                                              (renMaster: RenMaster_NewServer 0x00160bc8)
         → Mem_New(size); ctor; vptr                        sizes/ctors/vtables: analysis/servers.tsv
     → this->vslot22(obj, data) = Mgr_InitChild             0x0027f920
         → obj->vslot2()                                     Server init: g_ServerTable[(type>>16)&0xfff] = obj
 → this->vslot6(obj) = Mgr_AddChildSorted                    0x00272fd8
 → WadTag_Object: name → dictionary; link into group
```
Master_Server itself: type `(5<<16)|0` routes to the root (entry 0), whose vtable 0x002f6968 has
the same slot 10/5 bodies (`Root_CreateFromRecord` 0x0027bb10, `Bank_Create`-identical slot 5),
slot 19 `Root_NewMasterServer` 0x00187048 (0x34 bytes, vptr 0x002f4808) and slot 22
`Root_InitChild` 0x0027bac0 → Master vslot 2 = `Server_RegisterSelf` 0x00277118
(`g_ServerTable[(type>>16)&0xfff] = this` → entry 5).
Server init methods recovered: `Server_RegisterSelf` (Master, renMaster),
`TextureServer_Init` 0x00288b10, `GOServer_Init` 0x0028bcf0 (both: table store, default bank).
- Validation: breakpoint 0x00277118 (and the per-class init methods): `a0` is stored at
  `0x00362c48 + 4*((a0->type_word >> 16) & 0xfff)`.

### C-C5 Server factories
`Master_NewServer` 0x00185be0 switches on `(type >> 16) & 0xfff` (21 cases) and
`RenMaster_NewServer` 0x00160bc8 on the same field (0x0f, 0x11, 0x17, 0x1b, 0x20; it also
stores the result at renMaster+0x34/+0x3c/+0x38/+0x40/+0x48). Sizes, constructors and final
vtables are in `analysis/servers.tsv`; the constructor chains were found by scanning each
constructor for vtable installs (`tools/server_profile.py`).

### C-C6 Master keeps children sorted and updates them in that order
`Mgr_AddChildSorted` 0x00272fd8 inserts `child+8` into the circular list at Master+0x24,
**descending by `child+0x10`**, sets `child->flags |= 4`, increments Master+0x30 and stores the
node in Master+0x2c. `child+0x10` is the boot record's 5th word (the `t1` argument).
`Mgr_UpdateChildren` 0x00277200 (Master vslot 3, called every frame by
`Engine_UpdateServers`) walks the list from the head and, for each child without flag 0x10,
calls `this->vslot8(child)` if `child->flags & 3`, else `this->vslot7(child)`; Master vslot 7
(`Mgr_UpdateChild` 0x0027f8f8) calls `child->vslot3()`; Master vslot 8 (0x0027f9d0) calls
`Mgr_RemoveChild(this, child)` 0x00273190 and returns 0 — so flag bits 0/1 mean "remove from
the list at the next update" (the remover clears bits 0 and 2 and calls `this->vslot9(child)`
when bit 1 was set).
Ghidra's decompilation drops the `child` argument; the disassembly shows `a1 = node - 8`
(0x00277228..0x00277234).
- Validation: walk the list at `*(0x00362c48+0x14) + 0x24` after boot; `+0x10` of successive
  children must be non-increasing.

### C-C7 Pooled servers
16 server classes build `PooledServer` (vptr 0x002f68b0 before the derived one): buckets of
object pointers (+0x24, count +0x40 = 1, current +0x44), an array of `arg_a3` u32s (+0x34),
a context stack (+0x48, top index s8 at +0xc8, initially −1), the default-bank index (+0xd4).
`TextureServer_Init`/`GOServer_Init` (vslot 2): register in the table, create the default
bank with vslot 22, put it in the pool (index → +0xd4, bank+6, bank+0x30 = server), call
bank->vslot2.
`PooledServer_CreateFromRecord` 0x0027c0b0 (vslot 10 of all pooled classes):
`bank = pool[this->vslot19(data)]; bank->vslot10(hdr, data)`.
`PooledServer_SelectBank` 0x00288f40 (vslot 19 of 15 classes; GO 0x0028c020 and Wad
0x0028bc78 identical): instance record (data word0 < 0) → `default_bank_index`; otherwise
`ctx_stack[ctx_top]->pool_index` (+0x1c).

### C-C8 Context stacks
`GOServer_PushContext` 0x0027cb48 (vslot 16): `ctx_stack[++ctx_top] = ctx`, then for every
component in `ctx+0x80` owned by another server, `owner->vslot16(component)`.
`GOServer_PopContext` 0x0027cc00 (vslot 17) mirrors it with vslot 17.
`WadServer_GetCurrent` 0x0027ce10 (vslot 18): `ctx_top < 0 ? 0 : ctx_stack[ctx_top]`.
`WadServer_Push` 0x0027d6b0 also calls `FUN_0014b778(wad+0x9c)` and `FUN_00181748(wad+0x4c)`
before pushing.

---

## D. Objects and WAD → object paths

### C-D1 Object header idiom
Constructors start with: `+0x00 = (data[0] & 0x0fffffff) | 0x40000000; (u16)+0x04 = 0x20;
+0x08 = +0x0c = 0`. Classes built with the base-class idiom continue: `+0x10 = 0xDEADBEEF`
(then `data[4]`), `+0x14 = data[3]`, `+0x18 = data[2]`, `+0x1c = -1`, `+0x20 = vptr`
(`RecordData_Passthrough` 0x0027b9a0 returns its second argument, so `+0x14/+0x18` come from
the record). Seen in ≥100 constructors (all installers of 0x002f6a48) and in
`Texture_ctor` (first 0x10 bytes only).

### C-D2 `WadTag_Object` (0x00185748)
1. `existing = Obj_FindByName(hdr->name)`.
2. `size == 0`: if `existing`, link it under the current group parent (`Obj_AppendToChildList`).
3. Otherwise `obj = g_ServerTable[*(u16*)data]->vslot10(hdr, data)`.
4. If `obj` and the name is non-empty and does not start with a space: when
   `obj->type_word` has bit 31 set, register only if its low half is 1. Register with
   `Dict_Replace` if the name exists, else `Dict_Insert`. If owner = 3 and subtype = 1, also
   `FUN_0018f128(WadServer->vslot18(), obj)`.
5. Link `obj` under `g_WadGroupStack[g_WadGroupTop]` if `g_WadGroupTop < 0x10`.
6. If a GroupStart is pending: clear it, `--g_WadGroupTop`, store `obj`.

### C-D3 GroupEnd and ActivateByName
`WadTag_GroupEnd` 0x00185978: `p = g_WadGroupStack[g_WadGroupTop]; owner(p)->vslot11(p, hdr);
++g_WadGroupTop`.
`WadTag_ActivateByName` 0x001859f0 (tags 5 and 0x13): `o = Dict_Lookup(Dict_GetCurrent(),
hdr->name); r = owner(o)->vslot5(o); owner(r)->vslot16(r)`.

### C-D4 Streaming loader and payload ownership
`Wad_StreamUpdate` 0x0018cf70 reads 0x20-byte headers from the stream object
`*0x002d8168` (created in `Wad_InitLoader` with a 0x40000-byte buffer) into 0x0032f758.
A payload that is contiguous in the buffer is dispatched in place. Otherwise it is copied into
`FUN_0014b508(heap, size, 0x40)` (except tags 0xf/0x10) and `hdr->param |= 0x2000`. After
dispatch the block is freed unless `hdr->param & 0x4000`.
Creators that take the block when `param & 0x2000` is set: renModel 0x00165710 (sets 0x5000),
AnimServer bank 0x00119098 (sets 0x5000), SoundServer bank 0x0017fcf8 (sets 0x1000 for subtypes
0/3/4). When the bit is clear they copy the payload into their own heap block.
- Validation: watch `0x0032f75a` (param) at 0x0018d2c0; when `0x4000` is set the block at
  `*0x002d8138` must still be referenced by the created object.

### C-D5 Texture path
```
TXR_ record (88 bytes, type 7)
 → WadTag_Object → g_ServerTable[7] (TextureServer, vtable 0x002f2930)
 → PooledServer_CreateFromRecord 0x0027c0b0 → PooledServer_SelectBank 0x00288f40 → current bank
 → bank->vslot10 = TextureBank_CreateFromRecord 0x00291a60 (class 0x002f7250)
     obj = vslot20 = TextureBank_NewTexture 0x00291ae8: requires subtype 0; Pool_Alloc(bank+0x48);
           Texture_ctor 0x001745d8(obj, data)
     vslot22(obj, data); vslot6(obj)
```
The individual functions are confirmed. That the active bank is a 0x002f7250 object at runtime
is inferred (H-D3).

### C-D6 GameObject-server functions
- `GOServer_ctor` 0x00142888: pooled ctor + vptr 0x002f6470; `g_SeqIds[0..33] = -1`;
  `SeqId_Assign(1)`.
- `GOServer_NewDefaultBank` 0x0027ca28: `new(0x3c)`, type `0x00010001`, vptr 0x002f6530.
- Default bank vslot 10 `GOBank_NewDescriptor` 0x0027c820: `new(0x54)`, no vptr; type
  `((data>>16 & 0xfff) | 0x8000) << 16 | (data & 0xffff)`; copies `data[2..5]`, two 24-byte
  names (`data+0x18`, `hdr->name`) to +0x1c/+0x34, `data+0x30` to +0x50.
- Default bank vslot 19 `GOBank_NewObject` 0x0027c9a8: `new(0xa4)` + `GoClassA4_ctor`.
- `GoClassA4_ctor` 0x00140248: vptrs 0x002f6a48 → 0x002f66f0 → 0x002f6610; list head at +0x80;
  `type_word = (type & 0xf000ffff) | 0x80000000 | (u16)data[0] << 16`; `+6 = 0xffff`;
  three pools from the root WAD (follow `+0x1c0` links from `WadServer_GetCurrent()`) with
  element sizes 0x120/0xa0/8 → +0x8c/+0x90/+0x94; `+0x98 = data[0x14]`.
- `GoClassA4_CreateFromRecord` 0x001404f0 (vtable 0x002f6610 slot 10): subtype 0x2a → copy
  payload to `this->wad+0x1dc`; subtype 1 → 0x40-byte object; otherwise 0x70-byte object
  with 0x48 bytes copied from `data+0x1c` to `+0x20`, linked to a parent named at `data+4`
  unless subtype is 3.
- `GoClassA4_NewInstance` 0x00140450: `Pool_Alloc(this+0x8c)` + `GoInstance120_ctor`.

### C-D7 Material record handling is deferred
`MatBank_CreateFromRecord` 0x00162fa0 copies `hdr->name` to 0x002fd5f0 and calls
`Wad_StashRecord(data, hdr->size)` 0x0018d758, which copies the payload to 0x0036a718, stores
the size at 0x0036a818, sets 0x002d8174 = 1, and returns 0x0036a718. That buffer is what
`WadTag_Object` receives as the "object".

### C-D8 GO default bank per-frame update (`FUN_00283510`, vtable 0x002f6530 slot 3)
_Static, read instruction by instruction (`tools/eedis.py dis 0x283510`). Runtime: pending._
- On entry it asks the WadServer for its current context: `g_ServerTable[0x16]` (`lw 0x2ca0` from
  `0x00360000`) with vslot 18 (`+0x90/+0x94`). Call the result `cur_wad`.
- It walks the bank's list at `+0x24`. The list is **NULL-terminated** (`beqz s2`, unlike
  Master's circular list), and each node is `obj+8`.
- For each object:
  - If `flags & 0x10`, skip it.
  - Else if `flags & 3`, call `this->vslot8(obj)` (remove).
  - Else, when `obj+0x70 != 0`:
    1. If `obj+0x88` (the WAD at creation, C-D6) differs from `cur_wad`: pop the previously pushed WAD if any (WadServer vslot 17), then push the object's WAD with `g_ServerTable[wad->type & 0xffff]->vslot16(wad)`.
    2. If the object's own server does not have the object as its current context (vslot 18), push it (vslot 16).
    3. Call `obj->vslot3()` (`+0x18`).
    4. Pop the object's context if it was pushed (vslot 17).
- After the loop, pop the last pushed WAD.
- Consequence: the slot numbers for context access are confirmed on the WadServer and GOServer
  paths: vslot 16 = push, vslot 17 = pop, vslot 18 = current (cf. C-C8).

### C-D9 GoClassA4 slot 2 and slot 3: the node tree
_Static. Runtime: pending._
- **Slot 2 `FUN_00283410`:**
  - `+0x24 = this->vslot14(0)`.
  - Allocates a 0xc0-byte root node from the current heap (`FUN_0014b328(heap, 0xc0, 0x10)`) and constructs it with `FUN_0013bf50(node, this+0x24 result)`.
  - Sets `node+0xb0 = 0` and `node+0xb4 = new 8-byte list` (`FUN_0027ba00`).
  - Pushes the node on a per-context node stack: entries at `+0x2c`, s8 top at `+0x6c` (the ctor sets it to 0xff, C-D6 `field_6c`).
- **Slot 3 `FUN_00140ff8`:**
  - Does a depth-first walk from `this+0x28` (the root node) through the child lists at `node+0xb4`. Each list cell is `{next, ?, node}`, the node is at cell `+8`, and the list is circular with the list object as sentinel. The walk keeps an inline iterator with a 16-entry (head, cursor) stack.
  - It collects every descendant with `flags & 3` and not `flags & 0x10` into a 64-entry stack array. There is **no bounds check** on that array.
  - It then calls `this->vslot8(node)` for each collected node.
  - It does **not** call any per-node update.

### C-D10 GO node layout (base ctor `FUN_0013bba0`, `FUN_0013bf50`, `GoInstance120_ctor`)
_Static. Runtime: pending._
- **Base node (`FUN_0013bba0(node, rec)`):**
  - `+0 = (rec[0] & 0x0fffffff) | 0x40000000`, `+4 = 0x20` (flags), `+8/+0xc = 0` (link).
  - `+0x10 = 0xdeadbeef`, `+0x14` = sub-object (`FUN_0027c2d8`), `+0x18 = 0`, `+0x1c = rec`.
  - `+0x20..+0x5f` = 4×4 identity (built from `vf0`).
  - `+0x60 = +0x62 = 0xffff`.
  - `+0x68 = ++g_counter@0x002d7978` when `+0x60 == 0xffff`, otherwise 0 (64-bit store).
    _Runtime (`runtime-validation.md` `ingame1`→`ingame2`): the value is refreshed whenever the matrix
    changes. It is a version stamp, not a fixed id._
  - _Runtime: `+0x20..+0x5f` is the node's world transform (Kratos's position/yaw tracked his
    movement). `+0x108`/`+0x10c` are a message handler and its context (`FUN_0013d350/360/378`)._
- **`FUN_0013bf50`:** the base node plus a second 4×4 identity at `+0x70..+0xaf`. It is the full constructor of the 0xc0 root node.
- **`GoInstance120_ctor` 0x0013c490** (0x120 pool element):
  - Starts with `FUN_0013bf50`.
  - `+0xb0 = +0xb4 = 0`.
  - 13 sub-objects at `+0xc0..+0xf3` (`FUN_0027dd50`).
  - `+0xf4 = +0xfc = +0x108 = +0x10c = 0`; `+0xf8` is a u16 with bit 0x10 set.
  - `+0x104` = optional 0xa0-byte object from the context's pool `+0x90`, built by `FUN_001435b8(obj, node, rec)`. It is created when `rec[0xf] & 2`; for records whose subtype is not 1, the record is taken from `rec+0x6c` and `rec+0x22` is copied to `+0xf8`.
- **The nodes have no vptr.** `+0x20` holds matrix data, not a vtable, so they are not `PolyObject`s.

---

## E. Compiler and ABI facts

### C-E1 Virtual-call ABI
Every virtual call in the engine core is `lw vt,0x20(obj); lh d,8n(vt); lw f,8n+4(vt);
addu a0,obj,d; jalr f`. Vtable entries are 8 bytes `{s16 delta; s16 index; u32 fn}` with an
all-zero entry 0. `tools/vtables.py` finds 231 such tables in `.rodata`; `index` is 0 in all
of them and `delta` is 0 in every entry of the core classes. All recovered classes keep the
vptr at +0x20.

### C-E2 Constructors install base vtables first
`Root_NewMasterServer` 0x00187048 stores 0x002f6a48 then 0x002f4808; pooled-server
constructors store 0x002f6a48, then 0x002f68b0, then the derived table (e.g. `GOServer_ctor`);
`GoClassA4_ctor` stores three in sequence.

### C-E3 Destructors take a delete flag
`BaseObject_dtor` 0x0027b9a8 (slot 1 of 0x002f6a48) sets the vptr back to 0x002f6a48 and calls
`Obj_DeleteIfInChrg(this, flags)` 0x0027b948 = `if (flags & 1) Mem_Delete(this)`.
`FUN_001002b8` calls it with flags 0 (base sub-object destruction).

### C-E4 Pure-virtual slots
11 slots of 0x002f6a48 (2–6, 8–13) and slots 12, 13, 19 of 0x002f68b0 point to 0x002bff80 =
`j 0x002c1500`, which calls the function pointer stored at 0x002db8f0
(default 0x002c14f0 → `FUN_002c7820`, which does not return). Across all recovered vtables,
21 tables contain 84 such slots.

### C-E5 Global constructors
`main` first calls 0x002bf810: once-flag at 0x0035ad40, then `j 0x002bf760`, which reads the
list at 0x002de0ac: if word 0 is −1 count non-zero entries, else use word 0 as the count; then
call entries from last to first. The list is `{2, 0x002a7728, 0x002d76f8, 0}`. Entry 0x002a7728
calls `FUN_00278100(1, 0xffff)`.

### C-E6 Calling convention
- Integer arguments in `a0–a3, t0–t3` (8 registers): `BootWad_AddServer6` reads args 5–8 from
  t0–t3 (0x0018973c..0x00189750); across 19,956 `jal` sites, t0 is loaded before the call 1,301
  times, t1 899, t2 282, t3 162.
- Float arguments in `f12–f19`, more on the stack: `FUN_001892d0` copies f12–f19 into
  callee-saved registers and loads the 9th/10th float from 0xb0/0xb8 of the caller's frame;
  `Boot_CreateServersAndEngineResources` stores those two to (sp) and 8(sp) before the call.
- Returns in v0 (`Wad_DispatchRecord`), f0 (`FUN_0018b8c0`).
- Callee-saved GPRs are saved with 128-bit `sq`/`lq`, ra with `sd`/`ld`.
- All 4,940 stack frames are multiples of 16 bytes.

### C-E7 Code generation
- 0 `$gp`-relative loads, stores or address computations in `.text`, although `_start` sets
  gp = 0x00303df0.
- COP1 usage: 15,057 single-precision ops, 934 word conversions, **0 double-precision ops**.
- 282 `break 7` division-by-zero traps after `div` (`div; beql divisor,zero; break 7`).
- `Mem_New`/`Mem_NewArray` allocate 8-byte aligned; WAD payload blocks 0x40-aligned; model
  and sound blocks 0x10-aligned.

---

## F. Tooling facts

### C-F1 Ghidra decompiler problems found and fixed
- 516 distinct vtable-slot targets (of 3,253 slot entries) were not functions after
  auto-analysis; pass 3 created them (5,883 → 6,403 functions after re-export).
- Busy-waits on the interrupt-written `g_VBlankCount` were folded into infinite loops, deleting
  the rest of `FUN_0025a1f0` (including the server bootstrap call). Fixed in pass 4 by making
  0x0036a6a0, 0x002d8108 and 0x002d810c separate volatile memory blocks
  (`analysis/volatile.tsv`).
- 80 of the remaining 176 "removed unreachable" blocks are division-by-zero traps with a
  constant divisor (harmless). 96 blocks in other functions are unclassified (open issue).
- Ghidra drops arguments that were set before a loop and used as `a1` in virtual calls
  (e.g. `Mgr_UpdateChildren`); key functions have hand-checked pseudocode in
  `docs/engine-reconstruction.md`.
