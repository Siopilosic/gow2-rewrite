"""GOD OF WAR II GODOFWAR.TOC / PART{1,2}.PAK reader.

TOC layout (little-endian), verified against the US disc (SCUS-97481):
  u32 file_count
  file_count x { char name[24]; u32 size; u32 copy_count; u32 first_copy_index; }
  u32 copy_sector[]            # indexed by first_copy_index .. +copy_count
A file may be stored more than once (in either PAK) to reduce seek times.
copy_sector is a 2048-byte sector index: values < 10,000,000 address PART1.PAK,
values >= 10,000,000 address PART2.PAK at (value - 10,000,000).

Usage:
  python gowtoc.py <TOC> list
  python gowtoc.py <TOC> <iso> extract <out-dir> [name-glob]
"""
import fnmatch
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from iso9660 import Iso, SECTOR  # noqa: E402


def parse_toc(data):
    count = struct.unpack_from("<I", data, 0)[0]
    entries = []
    for i in range(count):
        off = 4 + i * 36
        name = data[off:off + 24].split(b"\0")[0].decode("latin1")
        size, ncopies, first = struct.unpack_from("<3I", data, off + 24)
        entries.append((name, size, ncopies, first))
    tbl_off = 4 + count * 36
    sectors = list(struct.unpack_from(f"<{(len(data) - tbl_off) // 4}I", data, tbl_off))
    return entries, sectors


PART2_BASE = 10_000_000


class PakStream:
    """PART1.PAK / PART2.PAK addressed by TOC sector values."""

    def __init__(self, iso_path):
        self.parts = []  # (iso, entry)
        layers = Iso.layers(iso_path)
        for name in ("PART1.PAK", "PART2.PAK"):
            for iso in layers:
                try:
                    self.parts.append((iso, iso.find(name)))
                    break
                except FileNotFoundError:
                    continue

    @staticmethod
    def part_of(sector):
        return 2 if sector >= PART2_BASE else 1

    def read(self, sector, size):
        part = self.part_of(sector)
        iso, e = self.parts[part - 1]
        rel = sector - (PART2_BASE if part == 2 else 0)
        if rel * SECTOR + size > e.size:
            raise ValueError(f"sector {sector} outside PART{part}.PAK")
        iso.f.seek((iso.layer_base + e.lba + rel) * SECTOR)
        return iso.f.read(size)


def main():
    data = open(sys.argv[1], "rb").read()
    entries, sectors = parse_toc(data)
    if sys.argv[2] == "list":
        print(f"{len(entries)} files, {len(sectors)} copy sectors")
        for name, size, n, first in entries:
            locs = sectors[first:first + n]
            print(f"{name:<24} {size:>10}  copies={n}  sectors={locs}")
        return
    pak = PakStream(sys.argv[2])
    if sys.argv[3] == "extract":
        out = sys.argv[4]
        pat = sys.argv[5] if len(sys.argv) > 5 else "*"
        os.makedirs(out, exist_ok=True)
        for name, size, n, first in entries:
            if not fnmatch.fnmatch(name.upper(), pat.upper()):
                continue
            blob = pak.read(sectors[first], size)
            with open(os.path.join(out, name), "wb") as o:
                o.write(blob)
            print(f"extracted {name} ({size}) from PART{pak.part_of(sectors[first])}")


if __name__ == "__main__":
    main()
