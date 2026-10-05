# God of War II (SCUS-97481, NTSC-U v1.01) — file formats

Everything here was verified against the retail disc image. "Verified" means a
parser built from the description consumed every file of that type exactly, or
that decoded output was checked visually.

## Disc layout (DVD9, two ISO9660 volumes)

| Layer | Base sector | Contents |
|---|---|---|
| 0 | 0 | `SYSTEM.CNF`, `SCUS_974.81` (EE ELF), `IOPRP300.IMG`, IOP modules (`*.IRX`), `GODOFWAR.TOC`, `PART1.PAK` (4.26 GB) |
| 1 | 2083872 | `PART2.PAK` (4.27 GB) |

Layer 1 has its own primary volume descriptor at sector `layer0_blocks` (=
2083888); directory LBAs in layer 1 are relative to base `2083872`.
`SYSTEM.CNF`: `BOOT2 = cdrom0:\SCUS_974.81;1`, `VER = 1.01`, `VMODE = NTSC`.

## GODOFWAR.TOC — **verified** (913 entries, all copies byte-identical)

```c
u32 file_count;                                   // 913
struct { char name[24]; u32 size; u32 copy_count; u32 first_copy; } files[file_count];
u32 copy_sector[];                                // 1532 entries, to EOF
```
- Copies exist so the drive can read the nearest one; `copy_count` up to 18.
- `copy_sector < 10,000,000` → sector in `PART1.PAK`;
  `copy_sector >= 10,000,000` → sector `(v − 10,000,000)` in `PART2.PAK`.
- Content: 343 `.VPK`, 293 `.WAD`, 194 `.VAG`, 43 `.PSS`, 39 `.PSW`, 1 `.TXT`.

| Ext | Magic | Notes |
|---|---|---|
| WAD | `15 00 00 00` | level / resource container (below) |
| VAG | `VAGp` | Sony ADPCM audio |
| VPK | `VPK ` | streamed audio (to be documented) |
| PSS / PSW | `00 00 01 BA` | MPEG-2 program stream FMV (PSW = widescreen variants) |
| `R_MCICON.WAD` | `fd 2a 01 00` | different format, likely memory-card icon (TBD) |

## WAD — **verified** (292/292 files parse exactly to EOF; 277,483 object records)

> **Phase-2 update.** The WAD header, stride, tag-0 rule, tag handler table, and the TXR and instance payloads are now confirmed from the executable as well (see `confirmed.md` C-A3…C-A9). The tag names `HeaderEnd`/`WadHeader`/`PopHeap` below are labels taken from record names on the disc; tag 0x13 shares the handler of tag 5 and does **not** switch sections (`hypotheses.md` R7).

16-byte aligned record stream:
```c
struct WadRecord {
    u16  tag;
    u16  param;
    u32  size;        // payload length; for tag 0 it is the value itself
    char name[24];
    u8   data[];      // `size` bytes (none for tag 0), then pad to 16
};
```

| Tag | Name | Notes |
|---|---|---|
| 0x00 | Value | named integer, e.g. `HERO_HEAP_SIZE`, `MSGS_COUNT`, `EntityCount` |
| 0x01 | Object | payload starts with a u32 **type word** |
| 0x02 / 0x03 | GroupStart / GroupEnd | always paired (93,041 each) |
| 0x05 / 0x06 | context push / `PopContext` | |
| 0x07 | `MC_DATA`, `MC_ICONSYS` | memory card data |
| 0x08 | `MSGS_TXT` | message text |
| 0x09 | `MSH_*` mesh shapes | |
| 0x0b–0x10 | `DC_*` | six tags per WAD, meaning TBD |
| 0x12 | `RSRCS` | |
| 0x13 | HeaderEnd | ends the server-registration section |
| 0x15 | WadHeader | first record |
| 0x16 | `PopHeap` | |

**Type word**: `bit31` = server instance (header section), `bits 30..16` =
subtype, `bits 15..0` = **server id**. Server ids are exactly the ids the
executable registers in `Engine_RegisterServers` (see `engine.md`):

| Id | Server | Record prefixes |
|---|---|---|
| 0x01 | GOServer | `CXT_`, `go*` game objects |
| 0x03 | AnimServer | `ANM_` |
| 0x04 | ScriptServer | `SCR_`, `SCP_`, `ESC_` |
| 0x06 | LightServer | `LGT_`, `PSdirLite` … |
| 0x07 | TextureServer | `TXR_` |
| 0x08 | MatServer | `MAT_` |
| 0x09 | CameraServer | `CAM_` |
| 0x0C | GfxClutServer | `GFX_`, `PAL_` |
| 0x0F | renModelServer | `MDL_` |
| 0x10 | CollisionServer | `COL_`, `CDV_`, `CDZ_`, `VSV_`, `VSZ_` |
| 0x11 | renParticleSvr | `PRT_`, `PTC_` |
| 0x12 | WaypointServer | `WYP_` |
| 0x14 | BhvrServer | `BHV_`, `IO_CSM` |
| 0x15 | SoundServer | `SND_`, `SBP_` (bank), `SBI_`, `SEM_` (emitter) |
| 0x16 | WadServer | `WAD_` |
| 0x17 | renEEPrimSvr | `EEPR_` |
| 0x19 | EffectsServer | `FX_`, `FXC_` (subtypes 1..0xd) |
| 0x1B | renFlashServer | `FLP_` (Flash UI movies) |
| 0x20 | renShadowServer | `SHG_` |

## GFX / PAL images (server 0x0C) — **verified visually**

```c
u32 type;      // 0x0C
u32 width, height;
u32 encoding;  // 0 or 2 seen; both decode as linear pixels
u32 bpp;       // 4 or 8 for GFX, 32 for PAL
u32 count;     // 1 seen (mip/frame count?)
u8  data[];
```
- **Correction (2026-10-02, `hypotheses.md` R13).** 8-bpp pixel data with `encoding == 0` is stored **PSMT8-swizzled** (GS PSMCT32 block/column order), not linear. `encoding == 2` is linear. 4-bpp `encoding 0` decoded correctly as linear in the one case checked (`GFX_stripedBlocks`).
  - Checked visually:
    - RHOD10 `GFX_fishCapital` and `GFX_templePillarLow`: unswizzled = carved fish / ribbed stone; linear = noise.
    - R_SHELLA `GFX_fog`: unswizzled = clouds.
    - R_PERMA `GFX_decorChest01_gold` (enc 2): linear = chest; unswizzled = noise.
  - Implemented in `gow2-formats` `gfx::unswizzle8` and `tools/gfx_decode.py`.
- PAL: `width*height` RGBA8888 colours, alpha 0x80 = opaque (PS2 GS convention).
- 256-colour palettes are in GS **CSM1** order: swap bits 3 and 4 of the index.
- **Correction (2026-10-03, CONFIRMED visually).** The CSM1 remap applies to **every 8-bpp palette of 256 or more
  entries**, not only to palettes of exactly 256. Kratos's palettes have 512 entries (`PAL` 16x32, `count` 2: two
  sets, the first 256 used by `MAT_kratos1A`); the old `ncol == 256` rule skipped the remap for them and produced
  salt-and-pepper noise. With unswizzle plus CSM1 the face texture decodes to a clean image (scar, beard, armour
  trim). Which set `MAT_..B` uses is not decoded (both A and B currently use the first set).
  `tools/gfx_decode.py`, `gow2-formats/src/gfx.rs`, `tools/refresh_textures.py` (rewrites exported PNGs).
- ~~256-colour palettes are in CSM1 order only when the palette has exactly 256 entries.~~ (superseded above)
- `TXR_` objects (server 0x07) reference their `GFX_`/`PAL_` pair by name in
  24-byte fields.

## DC records (WAD tags 0x0b–0x10): **verified** (code + disc; details in `kratos-data.md` §6)

The six tags of a DC record all share the same record name (e.g. `DC_WAD_R_Hero`). Name offsets
are relative to the start of the record's payload.

```c
// 0x0b: 4 bytes (container creation)      0x0c: blob, self-relative pointers inside
struct Exports { u32 n; struct { u32 blob_off, name_off; } e[n]; };   // 0x0d, name -> "<name>_DC"
struct Imports { u32 n; struct { u32 blob_off, name_off; } e[n]; };   // 0x0e, patched to rel ptrs
struct Hashes  { u32 n; struct { u32 hash, name_off; } e[n]; };       // 0x0f, ignored by the game
struct Objects { u32 n; struct { u32 blob_off, name_off, type; } e[n]; }; // 0x10, ignored by the game
```
- **Hash:** `h = seed; for c: h = h*127 + toupper(c)`. Lookup key: `hash("_DC", seed = hash(name))`.
- **Pointers in the blob:** self-relative `s32` pointers (0 = null).
- **Packed lists:** `u32 = (s32 rel_off << 12) | count`.
- **Times:** half-floats.
- **Parser:** `tools/dcparse.py` (tables) and `tools/dcmoves.py` (move graph).

Not yet documented: models (`MDL_`/`MSH_`), animation, collision, scripts,
sound banks, Flash movies, VPK.
