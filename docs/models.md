# MDL_ models (renModelServer, id 0x0f)

_Started 2026-10-02 for the environment-recreation track (`environment-audit.md`)._

## Record handling: `FUN_00165710` (context slot 10). CONFIRMED (code)

- **Subtype 1** (`MDL_<name>_0`, the large ones):
  - the payload is copied to the heap with 16-byte alignment, or kept in place if `param & 0x2000`;
  - then `FUN_00168168(blob)` relocates the DMA addresses.
- **Other subtypes** (`MDL_<name>`, 88 bytes, subtype 2):
  - `Mem_New(0xb0)` + `FUN_00164328(obj, payload)`, a model instance object (type word 0x2000f);
  - pools sized by payload `+0x14`, `+0x18` and `+0x50`;
  - name copied to `+0x94`.

## Blob hierarchy: from the relocation routine `FUN_00168168`. CONFIRMED (code)

```
blob  u16 +0x08 nA;  u32 offA[nA] at +0x18          (relative to blob)
 A    u16 +0x02 nB;  u32 offB[nB] at +0x04          (relative to A)
  B   u16 +0x04 nC;  u32 offC[nC] at +0x08          (relative to B)
   C  s16 +0x00 kind; if kind == 0x18 or 0x0e:
        groups = u8 +0x18 * u32 +0x0c ; per group u32 +0x04 entries
        entries: 16 bytes each from +0x20; word +4 relocated by += C
```

## Packet contents (`tools/mdl_probe.py`; `MDL_hero_0` in R_HERO00)

- `MDL_hero_0`: 179,376 bytes, nA = 1, nB = 1, nC = 19 parts, all of kind 0x0e.
- **The entries are PS2 DMA tags** (`ref`, qwc, address; terminated by `ret`). HIGH: the standard encoding decodes cleanly.
- **The referenced data is VIF code.** The first packet of part 0:

```
STCYCL cl=1 wl=4
UNPACK V2-16  ×31 → VU addr 0x14 (+TOPS)
UNPACK V3-8   ×31 → 0x15
UNPACK V4-16  ×31 → 0x17
UNPACK V4-8 unsigned ×31 → 0x16
STCYCL 1/1
UNPACK V4-32  ×20 → 0x0
```

**Reading of the packet.** HIGH that this is per-vertex data interleaved 4 qwords per vertex, 31 vertices per packet. MEDIUM for which unpack holds which attribute:
- V2-16 = UV;
- V3-8 = normal;
- V4-16 = position (+ ADC/flag in w);
- V4-8u = colour;
- V4-32 = per-packet header (GIF tag / matrix index table).

Part header word `+0x1c` (e.g. `0x001f0003`) may hold vertex/packet counts. UNKNOWN.

## Vertex decode (`tools/mdl_decode.py`), 2026-10-02

**Batches.** Each DMA `ref` packet holds a sequence of vertex batches, triple-buffered in VU1 memory: bases 0x000/0x155/0x2ab plus `+TOPS`. CONFIRMED (hero survey: every packet repeats the pattern). Per batch:

| Unpack | Role | Evidence | Confidence |
|---|---|---|---|
| V2-16 | UV (≈ /4096) | value range | MEDIUM, not checked against textures |
| V3-8 signed | normal (length ≈ 127) | vector length | HIGH |
| V4-16 | position xyz; `w` = flags/extra | renders correctly (below) | HIGH |
| V4-8 unsigned | vertex colour, 128 = 1.0 | the hero's red tattoo and night-tinted Rhodes appear from vertex colour alone | HIGH |
| V4-32 ×N | per-batch header (GIF tag / matrix table?) | — | UNKNOWN |

**Strips.** Vertices form triangle strips; `w & 0x8000` means "no triangle", the PS2 ADC convention. HIGH: the previews have clean geometry.

**Scale.** Positions are fixed-point at **1/16**. HIGH: the blob's bounding sphere after the offset table (`r, cx, cy, cz`) equals the quantized extents ÷ 16 for `innerPillar`, `bridgeA` and `skydome`.

**DMA groups.** Only group 0 is used. Their meaning (LOD? matrix sets?) is UNKNOWN.

**Results**
- `MDL_hero_0` (R_HERO00): 6,951 vertices, 5,243 triangles, a recognizable Kratos (`analysis/models/hero_0.png`, `.obj`).
- `MDL_innerPillar_0` (RHOD10): a column with a sculpted capital.
- All 60 `RHOD10` models decode without error. Without the sky models they assemble into one coherent architectural scene in their stored coordinates (`analysis/levels/RHOD10_nosky/overview.png`, `level.obj`). `tools/level_export.py` does the export.

**Placement.** In the level WAD each model sits in a group `{MDL_x (88 B), MAT_* refs = material slots, MDL_x_0}`. A separate group `{go<name> (subtype 1), ref MDL_x}` binds it to a node. `gofishpillar` is a one-joint record (joint `innerPillar`) with an identity matrix. Instance (subtype 3) node transforms apply on top; the RAM nodes carry them (C-D10). Status: the binding is CONFIRMED from disc structure; whether the composed transforms are exact is not tested yet.

**Sky.** `MDL_skydome_0` / `MDL_starsSkydome_0` are centred on the origin (radius ≈ 2,112 after scaling) while Kratos plays at about (−1704, 3712, −5353). So the sky is most likely drawn camera-relative (MEDIUM).

## Instances and the two layers of a level (RHOD10)

**Server-1 records in RHOD10 by subtype:** 0 ×35, 1 ×234, 2 ×38, 3 ×227, 42 ×1.

- **Subtype 1** (`go<name>`, e.g. `gofishpillar`): a node or skeleton record. Its group also holds a zero-size reference to `MDL_<x>`, which binds the model to that node.
- **Subtype 2** (`go<name>refNN`, 104 bytes): an **instance reference**.
  - `+0x04` = name[24] of the subtype-1 node to instance;
  - `+0x20` = 3×3 rotation (9 floats, rows);
  - `+0x44` = an offset vec3;
  - `+0x50` = world position W vec3;
  - `+0x60`/`+0x64` = small ints.
  - RAM agreement (`ingame1`): `gofishpillarref01..07` (type 0x40020001) carry the offset in their first matrix (`+0x20` translation) and an area anchor (−1408, 3472, −5952) in their second matrix (`+0x70`).
- **Subtype 3** (104 bytes): marker nodes such as cameras and cutscene points (`gorhod10camsinit`, `gorhod10ccbegin`). They carry a position, with values next to Kratos's in-game position. **Extended 2026-10-02:** subtype 3 also places rig groups (`gochunks2`, `gofirstroom`): `+0x20` rotation rows, `+0x44` translation, `+0x50` world bounds centre. Rigged models use world = local · R + W (row vectors). See `animation.md`, go node placement.
  - ~~**Open (MEDIUM):** ref instances (subtype 2) use the same field layout, but the exporter applies their R as columns (`R · v`). The pillar-ring evidence below holds under either direction. For the six `gowindowref*` refs (listed under `gofirstroom`) the two conventions give mirrored arcs on the same 460-radius ring, so the viewer can't decide it. It needs the engine code that builds the instance matrix.~~ Resolved the same day, below.
  - **Ref rotation is row-vector. HIGH (RAM + VU code).** The built ref objects in `ingame1` (type 0x40020001, e.g. 0x1ce4f20) hold the record's R rows unchanged in a row-major 4×4 matrix: `+0x20` with the `+0x44` vector as row 3, and `+0x70` with the anchor (−1408, 3472, −5952) as row 3. Every VU program read so far transforms as `row3 + x·row0 + y·row1 + z·row2` (program A at VU 0x0eb8). So the engine computes `v · R + T`, and the exporter's `R · v` was the transpose. That matters only for the 31 of 38 RHOD10 refs that are rotated, and only in rotation direction (the ring radius test could not see it). Viewer A/B (RHOD10 grand hall, same camera): with `v · R` every arcade arch meets its pillars and the window sits inside its arch. With the old `R · v` the arch ribs twist off the pillars and the window slips out of its arch. Exporter fixed; all levels re-exported.

**Placement rule:** ~~`world = R · (v_model / 16) + W`~~ → `world = (v_model / 16) · R + W` (row vectors; corrected 2026-10-02, see subtype 3 above). HIGH: three refs put the pillars on one ring of radius ≈440 around the RAM anchor, and the exported grand hall renders coherently in the viewer (arched-window arcade, fish pillars, lion statues, mosaic floors). In RHOD10: 38 refs → 7 models (couchSection, innerPillar, hallArch, hallCeiling, hallWall, lionSection, window).

**Model record transform (88-byte `MDL_<n>`).** Resolved 2026-10-02.
- `+0x38` = offset vec3, `+0x48` = f32 **scale**, `+0x50` = u32 flag (1 only on `skydome` in RHOD10). Examples: bridgeA (−864, 3616, −7072)×4, templeTop (0, 4800, −7584)×2, innerPillar 0×1, skydome 0×4 with flag 1.
- The RAM model object (0xb0, type 0x2000f) holds the same values: `+0x08` offset, `+0x90` scale (`ingame1`: bridgeA −864/3616/−7072 and 4.0, skydome 4.0, innerPillar 1.0). CONFIRMED (disc = RAM).
- **Full placement:** `world = R · (scale · v/16 + offset) + W` for ref instances, and `scale · v/16 + offset` otherwise. HIGH: with it, the RHOD10 vista (bridges, colonnades, towers, water) and the ref-placed grand hall form one coherent palace in the viewer.
- **Corrected 2026-10-02 (R15):** the scale **divides**: `world = R · (v/16 / scale + offset) + W`. (Rotation order corrected later the same day: `(v/16 / scale + offset) · R + W`.)
  - Evidence: in 30+ single-joint rigs (`docs/animation.md`) the root joint matrix has translation equal to `+0x38` exactly and rotation rows equal to 1/scale (scale 4 → 0.25, 0.5 → 2.0).
  - Sizes: bridgeA spans 560 × 546 × 320 with the division but 8,954 × 8,729 × 5,121 with multiplication, against a level about 4,000 across.
  - In the viewer, RHOD10 becomes one coherent vista (city on the cliff inside the sky dome).
  - So `+0x48` is a quantisation factor. The line above is kept as the earlier reading; `tools/gltf_export.py --model-scale mul` reproduces it.
- ~~The flag-1 sky dome is drawn around the camera.~~ Withdrawn (R14): `+0x50` is set (1/2/3/4/0x14) on sky domes, but also on ropes, mirrors, lens flares and `ao*` actors. Its meaning is UNKNOWN. Skies are left at their stored position, which renders plausibly (e.g. PAL15 `DarkSkyDome` ×8).
- Other fields (survey of all 7,817 records, `mat_survey`-style dump in session):
  - `+0x14` 1–0x2a (part count?);
  - `+0x1c` 1–0x57;
  - `+0x20` ∈ {0, 8, 0x48, 0x18, 0x58};
  - `+0x2c`/`+0x34` one-bit mask and its complement (layer/visibility group?).
  - All UNKNOWN.

**Background vista** (first reading, superseded above). The other 53 models (skydome, bridges, templeTop, windowWall, ballistaWall, …) are bound to nodes whose matrices are all zero in RAM (`gobridgea`, `goskydome`, `gowindowhallb`). Those nodes belong to the `CXT_bv*` contexts (bvskydome, bvtemple, bvbridgeleft, bvwater, …).
- Hypothesis (MEDIUM): "bv" = background vista, drawn in its own camera-relative space.
- Drawing them camera-relative at full scale encloses the camera, so the real rule (scale, offset or a separate pass) is **unknown**.
- Next evidence: runtime, i.e. the renderer path for `bv` contexts or a GS frame dump.

## Materials and textures (`tools/textured_preview.py`, `tools/gfx_decode.py`)

- **Material slots** of a model are the `MAT_*` reference records between `MDL_<n>` and `MDL_<n>_0`, in order. Source: disc layout.
- **A part's slot** is the low 16 bits of part header word `+0x08`. HIGH: the pillar's 3 parts use slots 0/1/2 for its 3 materials; the hero's parts use slots 0–5.
- **`MAT_` (120 bytes):**
  - `+0x48` = TXR name;
  - `+0x08..+0x10` floats 0.8;
  - `+0x28` = 1.0;
  - `+0x30` = 1, 1, 0x44010080;
  - `+0x40` = 0x00feff7b, −1;
  - `+0x60..+0x70` = RGBA float tint.
  - The meaning of the other fields is UNKNOWN; only the TXR link is used.
- **UV scale is /4096.** HIGH: the textured pillar maps correctly; u ranges −0.25…1.75, i.e. tiling textures with wrap.
- **8-bpp enc-0 textures need PSMT8 unswizzling** (`formats.md` R13).
- **Small textures.** 8-bpp enc-0 images smaller than one 16×16 block (RHOD20 `GFX_IronStrap01` 8×32, data = w·h) are read linear. LOW: not verified visually.

### Material blend fields (`tools/mat_survey.py` → `analysis/materials/mat_survey.tsv`), 2026-10-02

Survey: 7,338 unique `MAT_` records from all WADs.

**`+0x38` (u32)**
- **Top byte = GS `ALPHA` selector** `A | B<<2 | C<<4 | D<<6`. HIGH: the name hints agree.
  - `0x44`: (Cs−Cd)·As+Cd, normal blend. 6,078 records.
  - `0x48`: Cs·As+Cd, additive. 1,184 records. Names: `*ADDITIVE*`, `AdditiveGlow`, `lavaFall`, glows.
  - `0x42`: Cd−Cs·As, subtractive. 76 records. Names: `pigeonSUBTRACTIVE`, `nothin_SUBTRACTIVE`, `Shadow_*`.
- **Byte 1.**
  - `0x09` on every additive and subtractive material, `0x01` on normal ones. So bit 3 = blended pass (MEDIUM).
  - `0x00` on 9 records (`defFogMat`, `sunflare`, `skyline*`, grass); its meaning is UNKNOWN.
- **Low byte.**
  - Bit 7 = textured. CONFIRMED by data: set on exactly the 6,912 MATs that have a TXR, and clear on all 221 without one. Untextured examples: `lambert1New*`, `VisShader*`, `EntShdr*`, `pticleMat`.
  - Bit 2 (`0x84`) is set mostly on `phong*` materials, possibly an env/specular pass. UNKNOWN.

**`+0x40` (u32)**
- `00feff7b` goes with normal blend; `00f6ff7a` with additive/subtractive.
- The two differ by `0x00080001`, likely z-write off for blended passes (MEDIUM).
- **Revised 2026-10-02 (rendering.md §1.1):** the low byte (`0x7b`/`0x7a`) is the VU1 shader-variant capability mask. The draw loop ANDs it with the model part's `+0x14` and ORs in `+0x10`. HIGH (code). The meaning of the upper bytes is still UNKNOWN.

**Other fields**
- `+0x3c`/`+0x44` are a mask pair (`0x100`/`~0x100`, `0x8`/…).
- `+0x74` ∈ {0, 1, 2}.
- `+0x60..+0x6c` look like an RGBA float tint (1.0 or 2.0 common).
- All UNKNOWN.

**In use:** `gltf_export.py` writes `extras.blend`/`textured`/`w38`/`w40`. The viewer renders additive/subtractive with matching blending and no depth write, and hides untextured MATs by default. In the viewer, BOG10's light shafts and PAL15's fire and storm glow now read correctly.

- **Result.** `analysis/models/rhod10_innerPillar_tex2.png`: the fish-carved capital, gold bands, stone block and shaft are all textured correctly.

## Next

1. ~~Decode one part to vertices; scale; strip rule~~ (done above).
2. ~~Textures / UV / material slots~~ (done above).
2. Confirm against the VU1 microprogram (renModelServer's MPG upload) and a PCSX2 GS frame.
3. Map `MAT_` / TXR references per part.
4. Repeat on a level WAD (`RHOD10`): models plus `go*` placements.
