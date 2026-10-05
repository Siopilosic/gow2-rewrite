"""Minimal ISO9660 reader for PS2 DVD images (handles >4GB images / dual layer).

Usage:
  python iso9660.py <iso> list
  python iso9660.py <iso> extract <path-in-iso> <out-file>
  python iso9660.py <iso> extract-all <out-dir> [--max-size BYTES]
"""
import os
import struct
import sys

SECTOR = 2048


class Entry:
    __slots__ = ("name", "lba", "size", "is_dir", "path")

    def __init__(self, name, lba, size, is_dir, path):
        self.name, self.lba, self.size, self.is_dir, self.path = name, lba, size, is_dir, path


class Iso:
    """layer_base: sector where this volume starts (0 for layer 0; for a PS2 DVD9
    layer 1 it is the sector 16 before layer 1's primary volume descriptor)."""

    def __init__(self, path, layer_base=0):
        self.f = open(path, "rb")
        self.layer_base = layer_base
        pvd = self.read_sectors(16, 1)
        if pvd[1:6] != b"CD001":
            raise ValueError("not an ISO9660 image")
        self.volume_id = pvd[40:72].decode("ascii", "replace").strip()
        self.volume_blocks = struct.unpack_from("<I", pvd, 80)[0]
        self.root = self._parse_record(pvd, 156, "")
        self.root.path = ""

    @classmethod
    def layers(cls, path):
        l0 = cls(path)
        out = [l0]
        size = os.path.getsize(path) // SECTOR
        base = l0.volume_blocks - 16
        if base + 17 < size:
            try:
                out.append(cls(path, base))
            except ValueError:
                pass
        return out

    def read_sectors(self, lba, count):
        self.f.seek((self.layer_base + lba) * SECTOR)
        return self.f.read(count * SECTOR)

    def _parse_record(self, buf, off, parent):
        ln = buf[off]
        lba = struct.unpack_from("<I", buf, off + 2)[0]
        size = struct.unpack_from("<I", buf, off + 10)[0]
        flags = buf[off + 25]
        nlen = buf[off + 32]
        raw = buf[off + 33: off + 33 + nlen]
        if raw in (b"\x00", b"\x01"):
            name = raw.decode("latin1")
        else:
            name = raw.decode("latin1").split(";")[0]
        path = f"{parent}/{name}" if parent else name
        return Entry(name, lba, size, bool(flags & 2), path)

    def listdir(self, d):
        data = self.read_sectors(d.lba, (d.size + SECTOR - 1) // SECTOR)
        out, off = [], 0
        while off < len(data):
            ln = data[off]
            if ln == 0:  # records never straddle sectors; skip to next sector
                off = (off // SECTOR + 1) * SECTOR
                continue
            e = self._parse_record(data, off, d.path)
            if e.name not in ("\x00", "\x01"):
                out.append(e)
            off += ln
        return out

    def walk(self, d=None):
        d = d or self.root
        for e in self.listdir(d):
            yield e
            if e.is_dir:
                yield from self.walk(e)

    def find(self, path):
        want = path.strip("/").upper()
        for e in self.walk():
            if e.path.upper() == want:
                return e
        raise FileNotFoundError(path)

    def extract(self, e, out_path, chunk=8 << 20):
        os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
        self.f.seek((self.layer_base + e.lba) * SECTOR)
        left = e.size
        with open(out_path, "wb") as o:
            while left:
                b = self.f.read(min(chunk, left))
                if not b:
                    raise IOError("short read")
                o.write(b)
                left -= len(b)


def main():
    layers = Iso.layers(sys.argv[1])
    cmd = sys.argv[2]
    if cmd == "list":
        for n, iso in enumerate(layers):
            print(f"layer{n} base={iso.layer_base} volume={iso.volume_id!r} blocks={iso.volume_blocks}")
            for e in iso.walk():
                kind = "<DIR>" if e.is_dir else f"{e.size:>12}"
                print(f"{iso.layer_base + e.lba:>9}  {kind}  {e.path}")
    elif cmd == "extract":
        for iso in layers:
            try:
                iso.extract(iso.find(sys.argv[3]), sys.argv[4])
                return
            except FileNotFoundError:
                pass
        raise FileNotFoundError(sys.argv[3])
    elif cmd == "extract-all":
        out = sys.argv[3]
        max_size = int(sys.argv[5]) if len(sys.argv) > 5 and sys.argv[4] == "--max-size" else None
        for n, iso in enumerate(layers):
            for e in iso.walk():
                if e.is_dir or (max_size and e.size > max_size):
                    continue
                iso.extract(e, os.path.join(out, *e.path.strip("/").split("/")))
                print(f"layer{n}: extracted {e.path} ({e.size})")


if __name__ == "__main__":
    main()
