"""Raw-encoding scans over .text of SCUS_974.81 (complements eedis xrefs, which only
see lui-pair constants).

  python eescan.py imm <hex16> [--op sw|lw|...]   every load/store/addiu with that 16-bit immediate
  python eescan.py vtable <addr> <n>              decode n gcc2 vtable entries {s16 delta; s16 index; u32 fn}
  python eescan.py words <addr> <n>               dump u32 words with string/func annotation
  python eescan.py jalr-offsets                   histogram of `lw vt, 0x20(obj)` virtual-call slot offsets
"""
import collections
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, decode, containing_func  # noqa: E402

OPN = {0x23: "lw", 0x2B: "sw", 0x21: "lh", 0x25: "lhu", 0x29: "sh", 0x20: "lb", 0x24: "lbu", 0x28: "sb",
       0x37: "ld", 0x3F: "sd", 0x1E: "lq", 0x1F: "sq", 0x09: "addiu", 0x31: "lwc1", 0x39: "swc1"}


def scan_imm(img, imm, only=None):
    lo, hi = img.text
    for a in range(lo, hi, 4):
        w = img.word(a)
        op = w >> 26
        if op in OPN and (w & 0xFFFF) == imm and (only is None or OPN[op] == only):
            _, m, ops = decode(img, a)
            f = containing_func(img, a)
            print(f"{a:08x}  {m:<6} {ops:<28} in {img.label(f) if f else '?'}")


def vtable(img, a, n):
    for i in range(n):
        delta, index = struct.unpack("<hh", img.bytes_at(a + i * 8, 4))
        fn = img.word(a + i * 8 + 4)
        print(f"  [{i:2}] +0x{i * 8:03x}  delta={delta:<4} index={index:<4} fn={img.label(fn) if fn else '0'}")


def words(img, a, n):
    for i in range(n):
        w = img.word(a + i * 4)
        s = img.cstr(w) if img.in_file(w) else None
        print(f"  {a + i * 4:08x}: {w:08x}  {img.label(w) if w in img.names else ''} {repr(s) if s else ''}")


if __name__ == "__main__":
    img = Image()
    c = sys.argv[1]
    if c == "imm":
        only = sys.argv[4] if len(sys.argv) > 4 and sys.argv[3] == "--op" else None
        scan_imm(img, int(sys.argv[2], 16), only)
    elif c == "vtable":
        vtable(img, int(sys.argv[2], 16), int(sys.argv[3]))
    elif c == "words":
        words(img, int(sys.argv[2], 16), int(sys.argv[3]))
