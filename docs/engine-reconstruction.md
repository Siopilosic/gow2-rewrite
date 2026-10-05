# God of War II engine — reconstruction (engine core)

Scope of this phase: startup → Master_Server → server registration → WadServer → GOServer →
objects → update loop. Other systems are described only as far as the binary has been read.
IDs in brackets refer to [`confirmed.md`](confirmed.md) (C-…) and
[`hypotheses.md`](hypotheses.md) (H-…). Function records with callers, callees and globals:
[`core-functions.md`](core-functions.md). Raw decompiler output for every confirmed/high
function: `analysis/core_decomp/`.

The pseudocode below was written from the disassembly and checked against Ghidra's output.
Where Ghidra's version is wrong (dropped arguments, folded loops), the pseudocode follows the
instructions.

---

## 1. Engine initialisation

```
_start (0x00100008)                                         [C-B1]
  clear .bss; SetupThread; SetupHeap; FUN_002d5008(); FlushCache(0)
  Mem_InitRootHeap()            heap "Root" = end of .bss .. 0x01ff7ff8   [C-B2]
  ei
  Exit(main(argc, argv))

main (0x001463b8)                                           [C-B3]
  __main()                      global constructors              [C-E5]
  FUN_001875e0(argc, argv, …); FUN_00187788(); FUN_00146178(); FUN_0018ae28()
  Engine_InitCore()             0x00187468                       [C-B4]
      FUN_0014d2c0()
      ServerTable_Init()        g_ServerTable[0..255] = 0; [0] = root object
      FUN_0018a490(0x0032f2d0)  installs VBlank (cause 2) and GS (cause 0) handlers
  FUN_001886b0()                (references "iop::InitialisePlugin")
  Engine_InitSubsystems()       0x00187498 → … → FUN_0025a1f0:
      Wad_InitLoader()          tag handlers, 256 KiB stream buffer
      repeat 60: wait for g_VBlankCount to change; FUN_00188e08()
      FUN_0018a580(...)
      Boot_CreateServersAndEngineResources()      ← builds the server tree (§2)
      … 13 × FUN_001400c8("SCR_…", fn)            (H-B4)
  Engine_MainLoop()             §6
  Engine_Shutdown()             (H-B2)
  return 0
```

## 2. Server architecture

The engine is a tree of *server* objects. Each server owns one class of engine objects and
is identified by a small integer id. Servers live in `g_ServerTable` (256 pointers at
0x00362c48) at the index of their id [C-C1].

```
root (table[0], 0x38 bytes, vt 0x002f6968)
└─ Master_Server (table[5], 0x34, vt 0x002f4808)
   ├─ WadServer 0x16      ├─ LightServer 0x06
   ├─ ProServer 0x0a      ├─ MatServer 0x08
   ├─ GOServer 0x01       ├─ TextureServer 0x07
   ├─ AnimServer 0x03     ├─ GfxClutServer 0x0c
   ├─ BhvrServer 0x14     ├─ SoundServer 0x15
   ├─ EvtServer 0x13      ├─ renMasterSvr 0x0d
   ├─ CollisionServer 0x10│  ├─ renModelServer 0x0f
   ├─ ScriptServer 0x04   │  ├─ renParticleSvr 0x11
   ├─ CameraServer 0x09   │  ├─ renFlashServer 0x1b
   ├─ WaypointServer 0x12 │  ├─ renShadowServer 0x20
   ├─ EffectsServer 0x19  │  └─ renEEPrimSvr 0x17
   │                      ├─ renPrimMaster 0x02
   │                      └─ EpiServer 0x0b
```

**Servers are created by WAD records** [C-C3, C-C4]. At boot,
`Boot_CreateServersAndEngineResources` writes a tag-1 record per server into a 2 KiB buffer
and dispatches it like any record from a file:

```c
// 0x00189728 (AddServer4 at 0x00189640 is the same with three u32s)
void BootWad_AddServer6(char *name, u32 parent, u32 id, u32 a3, u32 t0, u32 t1, u32 t2, u32 t3) {
    BootWad_BeginRecord(name, /*tag*/1, /*param*/0);   // u16 tag, u16 param, u32 size, name[24]
    emit_u32((id << 16) | parent);                       // type word
    emit_u32(0); emit_u32(a3); emit_u32(t0); emit_u32(t1); emit_u32(t2); emit_u32(t3);
    BootWad_EndAndProcess();      // patch size; BootWad_DispatchRecord(buf, buf + 0x20)
}
```

The record goes to `g_ServerTable[parent]` (Master, or renMaster for render servers), whose
generic create sequence ends in the factory:

```c
// Mgr_CreateFromRecord 0x0027f9f0 (Master/renMaster vslot 10; root's is identical)
PolyObject *Mgr_CreateFromRecord(Mgr *this, WadRecordHeader *hdr, void *data) {
    PolyObject *o = this->vslot5(data);        // Mgr_Create
    this->vslot6(o);                           // Mgr_AddChildSorted
    return o;
}
// Mgr_Create 0x00273290
PolyObject *Mgr_Create(Mgr *this, void *data) {
    PolyObject *o = this->vslot26();
    if (!o) { o = this->vslot19(data);         // Master_NewServer / RenMaster_NewServer
              this->vslot22(o, data); }        // Mgr_InitChild: o->vslot2()  (registers o)
    return o;
}
// Master_NewServer 0x00185be0 (excerpt)
PolyObject *Master_NewServer(Master *this, BootServerPayload *d) {
    switch ((d->type_word >> 16) & 0xfff) {
    case 0x01: return GOServer_ctor(Mem_New(0x11c), d);
    case 0x07: /* inline */ return pooled_ctor(Mem_New(0xd8), d, vt_TextureServer);
    case 0x13: return FUN_00121788(Mem_New(0x3834), d);          // EvtServer
    case 0x16: /* inline */ return wad_server_ctor(Mem_New(0xe0), d);
    ...                                                            // analysis/servers.tsv
    }
}
// Server_RegisterSelf 0x00277118 (Master/renMaster vslot 2); pooled servers do the same first
void Server_RegisterSelf(PolyObject *this) {
    g_ServerTable[(this->hdr.type_word >> 16) & 0xfff] = this;
}
```

Which index a server registers under and which server it reports to are two halves of one
word: `type_word = ((id<<16)|parent) & 0x0fffffff | 0x40000000`. The high half is the server's
own id, the low half (the parent) is its owner for dispatch.

## 3. Object / handle system

**There is no global object handle table** (H-O1; the pass-1 claim is rejected, R1).

* Every engine object starts with `ObjectHeader` (`type_word`, `flags`, `field_06`, list
  links). Polymorphic objects add `order_key`, `field_14`, `field_18`, `pool_index` and the
  vptr at +0x20 (`PolyObject`) [C-D1].
* The owning server is `g_ServerTable[type_word & 0xffff]`; methods are called on the
  server with the object as an argument [C-C2]:
  ```c
  #define OWNER(o)  g_ServerTable[*(u16 *)(o)]
  OWNER(obj)->vslot5(obj);
  ```
* Objects are held by pointer. Pooled servers keep them in bucket arrays indexed by a pool
  index (`PooledServer.buckets[cur_bucket].items[i]`); recycled indices come from the u32
  array at +0x34 (H-S3).
* Names: `WadTag_Object` registers objects in the current dictionary; `Obj_FindByName`
  searches the current dictionary, then the current WAD and its sub-WADs, then a global one.
* No null or range checks on table lookups were found (C-C2): an invalid id is a crash, not
  an error path.
* Thread interaction: none found. The VBlank/GS interrupt handlers touch only counters and
  GS_CSR (C-B6). Other threads (IOP RPC, sound) have not been examined.

```
type_word ──low16──► g_ServerTable[id] ──► server (vtable) ──vslotN(obj)──► behaviour
                                        └─► buckets[..].items[pool_index] ──► object
name ──► dictionary (current → WAD → global) ──► object
```

## 4. WAD loading

```
Wad_StreamUpdate (0x0018cf70, per call; time-sliced)          [C-D4]
  read 0x20-byte header → g_WadStreamHeader (0x0032f758)
  tag == 0 or size == 0      → Wad_DispatchRecordNoPayload(hdr)
  payload contiguous in buf  → Wad_DispatchRecord(hdr, ptr)
  otherwise                  → copy into heap block (align 0x40), hdr.param |= 0x2000,
                               dispatch when complete; free unless hdr.param & 0x4000

Wad_DispatchRecord (0x0018d6d8)                                [C-A3, C-A5]
  if (h = g_WadTagHandlers[hdr->tag]) h(hdr, data)
  return ((hdr->size + 15) & ~15) + 0x20
```

Tag handlers [C-A5, C-D2, C-D3]:

```c
// WadTag_Object 0x00185748 — tag 1
void WadTag_Object(WadRecordHeader *hdr, void *data) {
    void *existing = Obj_FindByName(hdr->name);
    void *obj;
    if (hdr->size == 0) {
        obj = existing;                                   // name reference
        if (!obj) return;
        if (g_WadGroupTop < 0x10) Obj_AppendToChildList(g_WadGroupStack[g_WadGroupTop], obj);
    } else {
        obj = g_ServerTable[*(u16 *)data]->vslot10(hdr, data);
        if (!obj) goto pending;
        if (hdr->name[0] != 0 && hdr->name[0] != ' ' &&
            (!(obj->type_word & 0x80000000) || (u16)obj->type_word == 1)) {
            if (existing && Dict_Contains(Dict_GetCurrent(), hdr->name)) {
                Dict_Replace(Dict_GetCurrent(), hdr->name, obj);       // skips the next step
            } else {
                Dict_Insert(Dict_GetCurrent(), hdr->name, obj);
                if ((u16)obj->type_word == 3 && ((obj->type_word >> 16) & 0xfff) == 1)
                    FUN_0018f128(g_ServerTable[0x16]->vslot18(), obj); // AnimServer subtype 1
            }
        }
        if (g_WadGroupTop < 0x10) Obj_AppendToChildList(g_WadGroupStack[g_WadGroupTop], obj);
    }
pending:
    if (g_WadGroupPending) { g_WadGroupPending = 0; g_WadGroupStack[--g_WadGroupTop] = obj; }
}
void WadTag_GroupStart(...) { g_WadGroupPending = 1; }                       // 0x00185968
void WadTag_GroupEnd(WadRecordHeader *hdr, void *d) {                       // 0x00185978
    void *p = g_WadGroupStack[g_WadGroupTop];
    OWNER(p)->vslot11(p, hdr);
    g_WadGroupTop++;
}
void WadTag_ActivateByName(WadRecordHeader *hdr, void *d) {                // 0x001859f0, tags 5/0x13
    void *o = Dict_Lookup(Dict_GetCurrent(), hdr->name);
    void *r = OWNER(o)->vslot5(o);
    OWNER(r)->vslot16(r);
}
```

### WAD → object paths

| Record | Path (static) | Status |
|---|---|---|
| server record (boot) | §2 | **confirmed** [C-C4] |
| `*X_*` instance record (e.g. `TXRX_R_Perm`, type 0x8000000N) | `WadTag_Object` → server vslot10 `PooledServer_CreateFromRecord` → `SelectBank` returns the **default bank** (bit 31) → default bank vslot10 (`Bank_NewDescriptor50` 0x00288f70 for 14 servers, `GOBank_NewDescriptor` for GO) | functions confirmed; class per server from `analysis/wad_object_paths.tsv` |
| `CXT_*` + tag 5 | descriptor → `GoClassA4` context → `GOServer_PushContext` | H-O2 (HIGH) |
| `go*` (type 0x30001) | GOServer → current context → `GoClassA4_CreateFromRecord` → 0x70-byte node | functions confirmed [C-D6]; selection H-O2 |
| `TXR_*` (type 7) | TextureServer → current bank → `TextureBank_CreateFromRecord` → `TextureBank_NewTexture` → `Texture_ctor` | **confirmed** functions [C-D5]; bank class H-W2 |
| `MAT_*` (type 8) | MatServer → current bank → `MatBank_CreateFromRecord` → payload stashed in 0x0036a718 | confirmed [C-D7]; real construction H-W1 |
| `MDL_*` (type 0x1000f/0x2000f) | renModelServer → bank vslot10 0x00165710: subtype 1 → model blob (adopt/copy), `FUN_00168168`; else `new(0xb0)` + `FUN_00164328` | creator read; bank selection H-W2 |
| `ANM_*` (type 3) | AnimServer → bank vslot10 0x00119098: payload copied to a 0x40-aligned block or adopted (param 0x2000 → 0x5000) and returned as the object | creator read [C-D4]; H-W6 |
| `SND_*`/`SBP_*`/`SEM_*` (type 0x15) | SoundServer → bank vslot10 0x0017fcf8: low nibble of subtype 0/3 → `new(0x3c)` `FUN_0017ee88`; 4 → `new(0x2c)` `FUN_0017f3c0`; else 0x58-byte object `FUN_0017cd00` | creator read; H-W4 |

Shared vs server-specific: tag dispatch, `WadTag_Object`, name registration, group linking,
`PooledServer_CreateFromRecord` and `*_SelectBank` are shared by all servers. Only the bank
class's vslot 10 (and the constructors it calls) is server-specific.

## 5. Resource system

Resources (textures, materials, models, …) are objects owned by **banks**. A bank is an
object in a pooled server's pool. Each server has a default bank, made in its init method,
plus the banks declared by every WAD's `*X_<wad>` instance records. A context stack per server
(`ctx_stack`, `ctx_top`) selects the bank that receives new records [C-C7, C-C8].
`WadServer_Push` pushes a WAD's heap and dictionary, then cascades vslot16 (push) to the
servers that own its components, so loading a WAD switches every server to that WAD's banks
at once. Name lookups resolve references between resources: `Texture_ctor` finds its
`GFX_`/`PAL_` objects by name in the current dictionary [C-A7].

## 6. Update / tick architecture

```c
void Engine_MainLoop(void)  { while (!g_MainLoopExit) Engine_Frame(); }   // 0x00187570
void Engine_Frame(void)     { Engine_UpdateServers(); FUN_0025ed60(); Time_UpdateFrameRate(); }
void Engine_UpdateServers(void) {                                          // 0x00187388
    RootObject *root = g_ServerTable[0];
    if (root->field_30)      root->vslot12();
    else if (root->field_34) root->vslot13();
    g_ServerTable[5]->vslot3();                       // Mgr_UpdateChildren on Master
}
void Mgr_UpdateChildren(Master *this) {                                    // 0x00277200
    for (node = this->child_head; node != &this->child_head; node = node->next) {
        PolyObject *c = (PolyObject *)((u8 *)node - 8);
        if (c->hdr.flags & 0x10) continue;
        if (c->hdr.flags & 3) this->vslot8(c);        // 0x0027f9d0: Mgr_RemoveChild(this, c)
        else                  this->vslot7(c);        // Mgr_UpdateChild: c->vslot3()
    }
}
// Mgr_RemoveChild 0x00273190: unlink c, --child_count, c->flags &= ~5,
//                             and if bit 1 was set: this->vslot9(c)
```

So bits 0/1 of `flags` mark a child for removal; it is unlinked during the next update walk
instead of being updated [C-C6].

Master's children are kept sorted by `order_key`, descending [C-C6], so a frame updates:
ProServer (0x7fff) → WadServer (0x7f0f) → AnimServer (0x7100) → BhvrServer
(0x7002) → EvtServer (0x7001) → CollisionServer (0x6200) → ScriptServer (0x6000) → GOServer
(0x5f00) → Camera/Waypoint/Effects (0x5e00) → Light (0x5d00) → Mat (0x5c00) → Texture (0x5b00)
→ GfxClut (0x5a00) → Sound (0x5901) → renMasterSvr (0x5900) → renPrimMaster (0x5700) →
EpiServer (0). Tie order and renMaster's own child loop are not yet verified.
A pooled server's update (`TextureServer_Update`, `GOServer_Update`) calls its default bank's
vslot 3.

## 7. Rendering architecture

Known: render servers are children of renMasterSvr and are built by `RenMaster_NewServer`,
which also keeps direct pointers to them (+0x34 model, +0x38 EE prim, +0x3c particle, +0x40
flash, +0x48 shadow) [C-C5]. renMaster runs late in the frame (order key 0x5900). The ELF
holds 23 `.DVP.overlay` VU microcode sections. Two engine materials ("stencil", "trail1") are
created at boot. Not analysed: renMaster vslot 3 (`FUN_00160cd0`), DMA/VIF packet building,
VU programs.

## 8. Animation architecture

Known: AnimServer (id 3, 0x4dc bytes, inline constructor, update key 0x7100) runs early in the
frame. Animation payloads become objects by being copied or adopted whole (§4). Records whose
owner is 3 and subtype 1 also go to `FUN_0018f128` on the current WAD. Not analysed: the
animation format and its evaluation.

## 9. Collision / physics architecture

Known: CollisionServer (id 0x10, constructor 0x0011f240, vtable 0x002f4a30, update key 0x6200);
bank creator 0x0011e5e8; disc records `COL_`, `CDV_`/`CDZ_` (types 0x10010/0x20010). Not
analysed beyond that.

## 10. Script architecture

Known: ScriptServer (id 4, ctor 0x0013fe48, update key 0x6000), bank creator 0x0013f520; native
callbacks registered by name (`FUN_001400c8("SCR_…", fn)`, H-B4); ~200 `SCR_*` names in
`.rodata`. ~~Not analysed beyond that.~~ First pass done 2026-10-03: see `docs/scripting.md` (registration, descriptor slots, 143 registered classes, script object layout).

## 11. Memory management

* `Mem_InitRootHeap` creates heap "Root" over all RAM between `.bss` and the stack, with a
  canary 0x0BADC0DE [C-B2].
* `Mem_New(size)` allocates from the *current* heap (`FUN_0014b6b0`) with 8-byte alignment;
  `Mem_Delete` frees [C-E7].
* Heaps are pushed and popped (`FUN_0014b778` / `FUN_0014b7a0`, MEDIUM): `Wad_InitLoader`
  wraps the stream allocation in one, and `WadServer_Push` pushes the WAD's heap (`wad+0x9c`),
  so allocations made while a WAD is current come from that WAD's heap.
* Fixed-size pools: `FUN_0014ad10` creates, `FUN_0014ae10` allocates, `FUN_0014af18` frees
  (MEDIUM). GO contexts take three pools (0x120/0xa0/8) owned by the root WAD [C-D6].
* WAD payloads: in place in the 256 KiB stream buffer, or a 0x40-aligned heap block whose
  ownership can pass to the created object (`param` 0x4000) [C-D4].
* Values `HERO_HEAP_SIZE`, `UPGRADE_HEAP_SIZE` are tag-0 records on the disc; their handler
  (0x0018d7c0) is not yet traced.

## 12. Open questions

1. Who sets `flags` bits 0/1 (mark-for-removal) on Master's children, and what vslot 9 does.
2. Root vslots 12/13 and root fields +0x30/+0x34 (H-S6).
3. ~~Tags 0, 6, 7–9, 0x0b–0x11, 0x15, 0x16 handlers.~~ Resolved: all traced (C-A10, C-D2/C-D3). Summary:
   - 0 = named int;
   - 6/0x14 = PopContext (vslot 17);
   - 7 = named string; 8 = named length-prefixed text; 9 = named blob (first wins);
   - 0x15/0x16 = open/close WAD scope (own heap and dictionary).
4. Where materials are really constructed (H-W1).
5. What sets `g_MainLoopExit` (H-B1).
6. The 0x120 GO instance class and when instances are spawned (H-O3).
7. renMaster's frame (vslot 3) and the render path.
8. Tie-breaking in `Mgr_AddChildSorted` for equal keys.
9. The 96 unclassified "unreachable block" removals in Ghidra output.
10. Runtime confirmation of every HIGH item (see the validation lines in `hypotheses.md`).
