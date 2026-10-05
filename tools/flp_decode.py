"""God of War II `FLP_*` Flash-like movie (renFlashServer, server 0x1b): layout walker.

The loader's pointer fix-up (FUN_00159670 and the helpers FUN_001590e8, FUN_00159128, FUN_001592d0, FUN_00159428, FUN_00159578, FUN_00159308,
FUN_00159380, FUN_00159348, FUN_00159540; docs/hud.md) lays the movie out as a header followed by arrays packed one after another, each aligned to 4
bytes, with the sub-arrays of an entry directly after the array that holds it. This script walks the file with that same algorithm and checks that the
last array ends exactly at the end of the record.

  python tools/flp_decode.py <file.WAD> [FLP_name]
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402


class Layout:
    """Walks a movie image and records where every array is. `cur` follows the loader's cursor."""

    def __init__(self, b):
        self.b = b
        self.cur = 0
        self.n = len(b)
        self.header = {}
        self.arrays = {}   # name -> (offset, count, stride)
        self.sub = []      # (parent name, index, name, offset, count, stride)

    def u16(self, o):
        return struct.unpack_from("<H", self.b, o)[0]

    def u32(self, o):
        return struct.unpack_from("<I", self.b, o)[0]

    def align(self):
        self.cur += (4 - (self.cur & 3)) & 3
        return self.cur

    def take(self, count, stride):
        """Reserve `count` records of `stride` bytes at the aligned cursor; returns the offset (0 for an empty array)."""
        o = self.align()
        self.cur = o + count * stride
        return o if count else 0

    # ---- the helpers, in the order the loader calls them ----
    def blob8(self, entry):          # FUN_001590e8: entry+6 = count of 8-byte items
        n = self.u16(entry + 6)
        return ("b8", self.take(n, 8), n)

    def blob_n(self, entry, size_off, wide):   # FUN_001592d0 (size u32 at +0x18) / FUN_00159348 / FUN_00159540 (u32 at +4)
        n = self.u32(entry + size_off)
        return ("bytes", self.take(n, 1), n)

    def g_item_a(self, entry):       # FUN_00159308: entry+6 = n, n * 10 bytes
        n = self.u16(entry + 6)
        return ("e10", self.take(n, 10), n)

    def g_item_b(self, entry):       # FUN_00159380: u16 at +6 = n entries of 8 bytes, each with a blob of u32 at +4 bytes
        n = self.u16(entry + 6)
        arr = self.take(n, 8)
        subs = []
        for i in range(n):
            subs.append(self.blob_n(arr + 8 * i, 4, False))
        return ("g_b", arr, n, subs)

    def g_struct(self, o):           # FUN_00159428 on the 0x18-byte struct at o
        n1 = self.u16(o + 10)
        a1 = self.take(n1, 8)
        s1 = [self.g_item_a(a1 + 8 * i) for i in range(n1)]
        n2 = self.u16(o + 12)
        a2 = self.take(n2, 0xc)
        s2 = [self.g_item_b(a2 + 0xc * i) for i in range(n2)]
        return {"a1": (a1, n1, s1), "a2": (a2, n2, s2)}

    def walk(self):
        b = self.b
        h = 0
        self.cur = 0
        self.align()
        self.cur = h + 0x5C
        c = {k: self.u32(h + o) for k, o in zip("ABCDEFG", range(0x38, 0x54, 4))}
        c["H"], c["I"], c["J"] = self.u16(h + 0x54), self.u16(h + 0x56), self.u16(h + 0x58)
        self.header = c
        # A: 4-byte entries
        self.arrays["A"] = (self.take(c["A"], 4), c["A"], 4)
        # B: 8-byte entries, each followed by a sub-array of 8-byte items
        ob = self.take(c["B"], 8)
        self.arrays["B"] = (ob, c["B"], 8)
        for i in range(c["B"]):
            self.sub.append(("B", i) + self.blob8(ob + 8 * i)[:3])
        # C: 0x24-byte entries
        oc = self.take(c["C"], 0x24)
        self.arrays["C"] = (oc, c["C"], 0x24)
        for i in range(c["C"]):
            e = oc + 0x24 * i
            flags, k = self.u16(e + 0x20), self.u32(e + 0x10)
            if flags & 2:
                a = self.take(k, 8)
                for j in range(k):
                    self.sub.append(("C.0", i, *self.blob8(a + 8 * j)))
            if flags & 4:
                a = self.take(k, 8)
                for j in range(k):
                    self.sub.append(("C.1", i, *self.blob8(a + 8 * j)))
            self.take(k, 2)
            if flags & 1:
                self.align()
                self.cur += 0x200
            else:
                self.take(k, 2)
        # D: 0x1c-byte entries, each with a blob of u32@+0x18 bytes
        od = self.take(c["D"], 0x1C)
        self.arrays["D"] = (od, c["D"], 0x1C)
        for i in range(c["D"]):
            n = self.u32(od + 0x1C * i + 0x18)
            self.sub.append(("D", i, "bytes", self.take(n, 1), n))
        # E: 0x20-byte entries
        self.arrays["E"] = (self.take(c["E"], 0x20), c["E"], 0x20)
        # F: 0xc-byte entries: a G-struct (0x18 bytes), then u16@+8 entries of 0x10 bytes each with a blob of u32@+4 bytes
        of = self.take(c["F"], 0xC)
        self.arrays["F"] = (of, c["F"], 0xC)
        for i in range(c["F"]):
            e = of + 0xC * i
            g = self.take(1, 0x18)
            self.sub.append(("F.g", i, "g", g, 1))
            self.g_struct(g)
            n = self.u16(e + 8)
            a = self.take(n, 0x10)
            for j in range(n):
                m = self.u32(a + 0x10 * j + 4)
                self.sub.append(("F.blob", i, "bytes", self.take(m, 1), m))
        # G: 0x18-byte entries
        og = self.take(c["G"], 0x18)
        self.arrays["G"] = (og, c["G"], 0x18)
        for i in range(c["G"]):
            self.g_struct(og + 0x18 * i)
        # the extra 0x18-byte struct at +0x20
        ox = self.take(1, 0x18)
        self.arrays["X"] = (ox, 1, 0x18)
        self.g_struct(ox)
        # H (0x14 bytes), I (8 bytes), J (bytes: the string pool)
        self.arrays["H"] = (self.take(c["H"], 0x14), c["H"], 0x14)
        self.arrays["I"] = (self.take(c["I"], 8), c["I"], 8)
        self.arrays["J"] = (self.take(c["J"], 1), c["J"], 1)
        return self.cur


def load(path, name="FLP_HUDA"):
    d = open(path, "rb").read()
    for _o, tag, _p, nm, body in wad_records(d):
        if tag == 1 and nm == name and body:
            return body
    raise SystemExit(f"{name} not found")


if __name__ == "__main__":
    b = load(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "FLP_HUDA")
    L = Layout(b)
    end = L.walk()
    print("size", len(b), "walker ended at", end, "->", "MATCH" if end == len(b) else f"off by {len(b) - end}")
    print("counts", L.header)
    for k, (o, n, s) in L.arrays.items():
        print(f"  {k}: offset {o:#x} count {n} stride {s}")
