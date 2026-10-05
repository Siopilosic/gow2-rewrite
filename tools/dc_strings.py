"""Dump the printable strings of every DC_* record (WAD tags 0x0b-0x10) of a WAD.

  python tools/dc_strings.py extracted/pak/R_HERO00.WAD

Writes analysis/dc/<WAD>/<record>_tag<NN>.txt (one string per line, with payload offset) and prints
a per-record summary. The record walk follows the confirmed WAD header/stride rules (confirmed.md
C-A3/C-A4); string extraction is a plain scan (>= 4 printable chars) and implies no structure.
"""
import os
import re
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STR = re.compile(rb"[\x20-\x7e]{4,}")


def records(data):
    off = 0
    while off + 0x20 <= len(data):
        tag, param, size = struct.unpack_from("<HHI", data, off)
        name = data[off + 8:off + 0x20].split(b"\0", 1)[0].decode("latin-1")
        body = 0 if tag == 0 else size
        yield off, tag, param, size, name, data[off + 0x20:off + 0x20 + body]
        off += 0x20 + ((body + 15) & ~15)


def main():
    path = sys.argv[1]
    data = open(path, "rb").read()
    wad = os.path.splitext(os.path.basename(path))[0]
    outdir = os.path.join(ROOT, "analysis", "dc", wad)
    os.makedirs(outdir, exist_ok=True)
    for off, tag, param, size, name, body in records(data):
        if not (0x0B <= tag <= 0x10):
            continue
        strs = [(m.start(), m.group().decode("ascii")) for m in STR.finditer(body)]
        fn = os.path.join(outdir, f"{name}_tag{tag:02x}.txt")
        with open(fn, "w", encoding="utf-8") as f:
            f.write(f"# {wad} record @{off:#x} tag {tag:#x} param {param} size {size}\n")
            for o, s in strs:
                f.write(f"{o:08x}\t{s}\n")
        prefixes = {}
        for _, s in strs:
            p = s.split("_", 1)[0] if "_" in s[:6] else s[:4]
            prefixes[p] = prefixes.get(p, 0) + 1
        top = ", ".join(f"{k}:{v}" for k, v in sorted(prefixes.items(), key=lambda kv: -kv[1])[:14])
        print(f"{name:20s} tag {tag:#04x} size {size:7d} strings {len(strs):5d}  {top}")


if __name__ == "__main__":
    main()
