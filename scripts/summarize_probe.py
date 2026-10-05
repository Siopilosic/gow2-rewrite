"""Reproduce the DIR hypothesis and readable report from the initial survey."""
import collections
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "research/reports/initial"
manifest = json.loads((OUT / "manifest.json").read_text(encoding="utf-8"))
files = manifest["files"]
main = next(f for f in files if f["path"] == "SLUS_212.15")
data = (ROOT / "WARRIORS.DIR").read_bytes()
n = struct.unpack_from("<I", data)[0]
assert len(data) >= 16 and (len(data) - 16) % 12 == 0
rows = list(struct.iter_unpack("<III", data[16:]))
wad_size = (ROOT / "WARRIORS.WAD").stat().st_size
hypothesis = {
    "model": "16-byte header; count u32le@0; records u32le[3]: candidate offset,length,unknown_08",
    "confidence": "HIGH_CONFIDENCE",
    "source_sha256": {f["path"]: f["sha256"] for f in files if f["path"] in ("WARRIORS.DIR", "WARRIORS.WAD")},
    "count_word": n, "rows": len(rows), "exact_size": len(data) == 16 + 12 * n,
    "ranges_in_wad": sum(o + length <= wad_size for o, length, k in rows),
    "aligned_2048": sum(o % 2048 == 0 for o, length, k in rows),
    "nonzero_sizes": sum(length > 0 for o, length, k in rows),
    "ordered_nonoverlap": all(rows[i][0] + rows[i][1] <= rows[i + 1][0] for i in range(len(rows) - 1)),
    "unique_word_2": len(set(k for o, length, k in rows)),
    "first_rows": rows[:4], "last_rows": rows[-4:],
    "wad_size": wad_size, "last_end": rows[-1][0] + rows[-1][1],
    "candidate_payload_bytes": sum(length for o, length, k in rows),
    "limitations": ["Alternate layouts not exhaustively tested", "No loader disassembly or runtime confirmation", "Identifier meaning unknown", "No member semantics parsed"]
}
(OUT / "dir-hypothesis.json").write_text(json.dumps(hypothesis, indent=2), encoding="utf-8")
e = main["elf"]
elf_bytes = (ROOT / "SLUS_212.15").read_bytes()
refs = []
for path in ["WARRIORS.WAD", "IOP/BFW.SND", "IOP/MUSIC.SND"]:
    token = ("cdrom0:\\" + path.replace("/", "\\") + ";1").encode("ascii")
    start = elf_bytes.find(token)
    if start >= 0:
        refs.append({"source": "SLUS_212.15", "source_sha256": main["sha256"], "offset": start,
                     "target": path, "kind": "LEXICAL_PATH", "confidence": "CONFIRMED",
                     "scope": "Exact matching path string; runtime use not verified"})
refs.append({"source": "SYSTEM.CNF", "target": "SLUS_212.15", "kind": "BOOT_CONFIGURATION", "confidence": "CONFIRMED"})
refs.append({"source": "WARRIORS.DIR", "target": "WARRIORS.WAD", "kind": "CANDIDATE_ARCHIVE_INDEX", "confidence": "HIGH_CONFIDENCE"})
(OUT / "references.json").write_text(json.dumps(refs, indent=2), encoding="utf-8")
inventory = "\n".join(f"| `{f['path']}` | {f['size']:,} | `{f['magic'].split(' ')[:4] and ' '.join(f['magic'].split(' ')[:4])}` |" for f in files)
sections = "\n".join(f"| `{s['name']}` | `0x{s['offset']:08X}` | `0x{s['addr']:08X}` | {s['size']:,} |" for s in e["sections"] if s["name"] and not s["name"].startswith(".DVP.overlay."))
hashes = "\n".join(f"- `{f['path']}`: `{f['sha256']}`" for f in files if f["path"] in ("SLUS_212.15", "SYSTEM.CNF", "WARRIORS.DIR", "WARRIORS.WAD"))
all_valid = all(s["file_range_valid"] for f in files if "elf" in f for s in f["elf"]["sections"])
report = f"""# Initial forensic report — The Warriors PS2 data

Date: 2026-09-17. Evaluation only. See [implementation plan](../../docs/evaluation-plan.md) and [database contract](../../docs/database-schema.md).

## Scope and preservation

Scanned **{manifest['file_count']} original files**, **{manifest['total_bytes']:,} bytes**. All files received full SHA-256 hashes. The source was opened read-only; generated material lives in research/. All file metadata remained stable during hashing: {all(f['stable_during_read'] for f in files)}. This does not independently authenticate a disc dump or guarantee immutability against concurrent same-size edits.

Strings and entropy use at most the first 8 MiB per file. The main ELF and DIR were read completely; the WAD and SND payloads were not fully semantically scanned. No archives were extracted, assets converted, code reconstructed or runtime tests performed. The initial probe is deliberately limited to this supplied corpus, not a hardened general scanner. Its keyword string hits include false positives.

## Game version

**CONFIRMED:** SYSTEM.CNF contains `BOOT2 = cdrom0:\\SLUS_212.15;1`, `VER = 1.03`, `VMODE = NTSC`. The named file exists and is an ELF executable. **HIGH CONFIDENCE:** this is the PS2 game dataset described by the user; the executable also retains Warriors source paths. **UNKNOWN:** pressing/re-release, language inventory, extraction provenance, modification history and exact retail revision. The configuration's VER value is not proof of game revision 1.03. The serial alone is not used here to authenticate a release.

## Directory inventory

| File | Bytes | First four bytes |
|---|---:|---|
{inventory}

## Executable and memory layout

**CONFIRMED:** `SLUS_212.15` is ELF32 little-endian, e_type=2, e_machine=8 (MIPS), entry `0x{e['entry']:08X}`, flags `0x{e['flags']:08X}`. This follows the [ELF header definition](https://www.sco.com/developers/gabi/latest/ch4.eheader.html). Machine number alone does not identify all EE instruction extensions.

One PT_LOAD segment maps file offset `0x1000` to virtual address `0x00100000`, with 4,812,652 file bytes and 6,380,380 memory bytes; flags=7. Its memory interval is `[0x00100000, 0x00715B5C)`. The tail beyond file-backed data is 1,567,728 bytes. There are 71 section headers, including 55 `.DVP.overlay.*` entries. File ranges of parsed ELF sections passed the probe's bounds checks: {all_valid}. This is header validation, not instruction validation.

| Section | File offset | Address | Bytes |
|---|---|---|---:|
{sections}

NOBITS sections describe memory rather than bytes to read from their listed file offsets. **CONFIRMED:** the main ELF has no section of type SHT_SYMTAB or SHT_DYNSYM. Source-path strings are present; they are not a recovered symbol table. Six supplied IRX files also have ELF32 little-endian headers. Their PS2 module-specific fields remain unanalysed.

**HIGH CONFIDENCE:** `.vutext`, `.vudata` and DVP section names indicate vector-unit-related material requiring separate interpretation. **PROBABLE:** `.gcc_except_table` is a GCC-family toolchain clue; compiler version is unknown. Entry-to-main path, function boundaries, calling conventions, globals, imports, RTTI/vtables and initialization call graph remain **UNKNOWN**. No reconstructed function names or addresses are invented.

## Middleware, asset and scripting evidence

Offsets below are absolute file offsets in SLUS_212.15, not function addresses. Exact strings are CONFIRMED observations; functional interpretations are scoped separately.

| Observation | Offset (decimal) | Interpretation |
|---|---:|---|
| Renderware Texture Dic | 4499464 | HIGH CONFIDENCE texture-dictionary handling lead |
| GBH Script | 4499584 | CONFIRMED label; format/VM relationship UNKNOWN |
| .../Core/ChunkSystem.cpp | 4500248 | HIGH CONFIDENCE chunk dispatch research lead |
| .../fileio/DVDWadIndexPS2.cpp | 4503024 | HIGH CONFIDENCE WAD loader research lead |
| .../fileio/RockWadIndexPS2.cpp | 4503224 | HIGH CONFIDENCE WAD index research lead |
| .../GameModes/InitLevel.cpp | 4526256 | HIGH CONFIDENCE level-loading research lead |
| .../Graphics/Devices/Renderware/DevRWGeneric.cpp | 4539600 | HIGH CONFIDENCE RenderWare integration |
| .../Scripting/ScriptLua.cpp | 4693928 | HIGH CONFIDENCE Lua-related integration; version/dialect/use UNKNOWN |
| .../Movie/BinkMovie.cpp | 4771056 | HIGH CONFIDENCE Bink playback integration |

**CONFIRMED:** all 16 PSS/*.BIK files begin `BIKi`; **HIGH CONFIDENCE:** these are Bink video containers, consistent with the independent [FFmpeg Bink demuxer](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/libavformat/bink.c) and executable strings. Headers/frame indexes and decoded video/audio have not been fully validated.

The first WAD bytes include repeated length-like fields, the bytes `0A 00 02 1C` at offset 0x28 and elsewhere, `PS2` at 0x54, and `civl_hl_br5` at 0x68. **PROBABLE:** a wrapped texture-related chunk is a useful first member candidate given the executable's texture-dictionary label. Chunk version, dimensions, swizzling, palettes and asset identity remain unverified. No RenderWare version is asserted from these bytes.

**PROBABLE:** IOP/BFW.SND and IOP/MUSIC.SND contain audio-related data, supported by their names and exact paths in the executable. Their first 32 bytes are zero. Codec, index layout, channels, sample rates and triggering remain UNKNOWN. MODULES/IOPRP300.IMG starts `RESET`; its internal image layout is UNKNOWN.

Maps, models, animation, collision, navigation, placements and individual mission files are **UNKNOWN** at this stage; no independent map file was identified. A Lua-related source path does not establish that all missions use ordinary Lua bytecode. Compression/encryption are **UNKNOWN** per member. Readable headers do not prove the entire WAD is uncompressed or unencrypted.

## Archive hypothesis and first format record

The DIR begins with little-endian 10701 (0x29CD). Its size exactly satisfies `16 + 10701 * 12 = 128428`.

| Offset | Size | Observed/candidate field | Confidence |
|---|---:|---|---|
| 0x00 | 4 | little-endian word=10701; candidate count | Value CONFIRMED, count HIGH CONFIDENCE |
| 0x04 | 12 | zero bytes in this sample; unknown_04 | Bytes CONFIRMED, semantics UNKNOWN |
| 0x10 + i*12 | 4 | candidate WAD byte offset | HIGH CONFIDENCE |
| 0x14 + i*12 | 4 | candidate stored byte length | HIGH CONFIDENCE |
| 0x18 + i*12 | 4 | unknown_08, unique across rows | Value CONFIRMED; identifier PROBABLE; filename hash SPECULATIVE |

Every one of the 10,701 candidate records has a positive length and a range inside WAD. All offsets are divisible by 2048. Ranges are ordered and non-overlapping. All third words are distinct. First row: offset=0, length=17,840, unknown_08=4090665195. Last range ends at 1,495,371,161, leaving 615 bytes after it. Inter-record gaps exist and have not been semantically classified as padding.

These are **CONFIRMED arithmetic results for the tested interpretation** and support a **HIGH CONFIDENCE index hypothesis**. They do not confirm ID semantics, compression, names, or that every record is one independent asset. No extraction success is claimed.

## Relationships

The boot configuration names the ELF. Exact strings in the ELF name WARRIORS.WAD, IOP/BFW.SND and IOP/MUSIC.SND. These are confirmed lexical relationships, not observed runtime reads. DIR → WAD is a HIGH CONFIDENCE candidate index relationship. See references.json for offsets and relationship types. Asset dependency graphs remain unpopulated until member interpretation.

## Hashes and verification

{hashes}

All 29 hashes, magic samples, entropy ranges, string samples and ELF metadata are in manifest.json. `unknown-files.json` conservatively includes every input without a confirmed type from the limited automatic probe, including text and video candidates whose interpretation is refined in this report. Empty per-file reference arrays in that early manifest are unpopulated; the supplemental references.json contains the actual relationship survey.

Reproduce locally from the dump root:

```text
python research/scripts/initial_probe.py . research/reports/initial
python research/scripts/summarize_probe.py
```

## Recommended next experiment

1. Implement the bounded candidate DIR index validator and test alternative layouts/units explicitly.
2. Independently sample WAD member headers across the table and validate nested boundaries.
3. Locate actual code references to the two WAD index source-path strings; establish lookup/read behavior and ID semantics.
4. Corroborate one read in the PS2 reference when runtime tracing is available; only then promote field meanings and extract a byte-identical slice.
5. Choose the first leaf asset by verified dependency simplicity. Texture data is a candidate, not a commitment.

Current progress: 29 files hashed; one main ELF's 71 section headers surveyed; 10,701 candidate index rows checked; zero game asset formats semantically completed; zero functions reconstructed or verified; total functions unknown. Native production tools, map viewer and game are NOT_STARTED. The evaluation plan is complete; the port is not.
"""
(OUT / "initial-forensic-report.md").write_text(report, encoding="utf-8")
print("Report written; candidate records:", len(rows), "; relationships:", len(refs))
