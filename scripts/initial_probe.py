"""Read-only, bounded initial survey. Not the proposed production WarriorsScan."""
import argparse
import collections
import hashlib
import json
import math
from pathlib import Path
import re
import struct


def strings(data, limit=20):
    return [{"offset": m.start(), "text": m.group().decode("ascii")[:240]}
            for m in list(re.finditer(rb"[\x20-\x7e]{6,}", data))[:limit]]


def elf(data, size):
    if data[:6] != b"\x7fELF\x01\x01":
        return None
    names = "type machine version entry phoff shoff flags ehsize phentsize phnum shentsize shnum shstrndx".split()
    result = dict(zip(names, struct.unpack_from("<HHIIIIIHHHHHH", data, 16)))
    result["program_headers"] = []
    result["sections"] = []
    errors = []
    for i in range(result["phnum"]):
        off = result["phoff"] + i * result["phentsize"]
        if result["phentsize"] < 32 or off + 32 > len(data):
            errors.append("Program table outside captured bytes or invalid stride")
            break
        row = dict(zip("type offset vaddr paddr filesz memsz flags align".split(), struct.unpack_from("<IIIIIIII", data, off)))
        row["file_range_valid"] = row["offset"] + row["filesz"] <= size
        result["program_headers"].append(row)
    for i in range(result["shnum"]):
        off = result["shoff"] + i * result["shentsize"]
        if result["shentsize"] < 40 or off + 40 > len(data):
            errors.append("Section table outside captured bytes or invalid stride")
            break
        row = dict(zip("name_offset type flags addr offset size link info addralign entsize".split(), struct.unpack_from("<IIIIIIIIII", data, off)))
        row["file_range_valid"] = row["type"] == 8 or row["offset"] + row["size"] <= size
        result["sections"].append(row)
    if 0 < result["shstrndx"] < len(result["sections"]):
        st = result["sections"][result["shstrndx"]]
        table = data[st["offset"]:st["offset"] + st["size"]]
        for row in result["sections"]:
            row["name"] = table[row["name_offset"]:].split(b"\0", 1)[0].decode("ascii", "replace")
    result["errors"] = errors
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    root, output = args.root.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # The dedicated research folder is excluded; no source file is written.
    files = sorted(p for p in root.rglob("*") if p.is_file() and
                   not p.is_relative_to(root / "research") and
                   not p.is_relative_to(output) and not p.is_symlink())
    records = []
    for p in files:
        before = p.stat()
        digest = hashlib.sha256()
        with p.open("rb") as f:
            sample = f.read(min(before.st_size, 8 * 1024 * 1024))
            digest.update(sample)
            while block := f.read(4 * 1024 * 1024):
                digest.update(block)
        after = p.stat()
        counts = collections.Counter(sample)
        entropy = -sum((n / len(sample)) * math.log2(n / len(sample)) for n in counts.values()) if sample else 0
        kind = "ELF32_LE" if sample[:6] == b"\x7fELF\x01\x01" else None
        suspected = kind or ("Bink video candidate" if sample[:3] == b"BIK" else "UNKNOWN")
        rec = {"path": p.relative_to(root).as_posix(), "filename": p.name,
               "extension": p.suffix, "size": before.st_size, "sha256": digest.hexdigest(),
               "magic": sample[:32].hex(" "), "confirmed_type": kind,
               "suspected_type": suspected, "confidence": "CONFIRMED" if kind else "HIGH_CONFIDENCE" if sample[:3] == b"BIK" else "UNKNOWN",
               "archive_membership": None, "references": [], "parser_status": "HEADER_ONLY" if kind else "UNANALYSED",
               "string_samples": strings(sample), "sample_bytes": len(sample),
               "sample_entropy_bits_per_byte": entropy,
               "stable_during_read": before.st_size == after.st_size and before.st_mtime_ns == after.st_mtime_ns}
        if kind:
            rec["elf"] = elf(sample, before.st_size)
            hits = []
            for m in re.finditer(rb"[\x20-\x7e]{6,}", sample):
                if re.search(rb"renderware|rw[A-Z]|rp[A-Z]|bink|zlib|inflate|deflate|\.wad|\.dir|\.snd|script|\.cpp|\.c\b", m.group(), re.I):
                    hits.append({"offset": m.start(), "text": m.group().decode("ascii")[:400]})
            (output / (p.name + ".strings.json")).write_text(json.dumps(hits, indent=2), encoding="utf-8")
        records.append(rec)
        print(rec["path"], rec["size"], rec["magic"][:47], flush=True)
    result = {"schema_version": 1, "scope": "Initial supplied files; excludes research/", "source_root": str(root),
              "file_count": len(records), "total_bytes": sum(r["size"] for r in records),
              "limitations": ["Full-file SHA-256; strings and entropy use first min(size, 8 MiB) only",
                              "No recursive archive parsing; empty references do not establish absence",
                              "ELF header fields parsed; instruction architecture extensions unverified",
                              "Bink candidate identification by prefix only"], "files": records}
    (output / "manifest.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    (output / "unknown-files.json").write_text(json.dumps([r["path"] for r in records if r["confirmed_type"] is None], indent=2), encoding="utf-8")
    for name in ["WARRIORS.DIR", "WARRIORS.WAD"]:
        with (root / name).open("rb") as f:
            data = f.read(512)
        (output / (name + ".header.txt")).write_text("\n".join(f"{i:08x}  {data[i:i+16].hex(' '):47}  {''.join(chr(c) if 32 <= c < 127 else '.' for c in data[i:i+16])}" for i in range(0, len(data), 16)), encoding="utf-8")


if __name__ == "__main__":
    main()
