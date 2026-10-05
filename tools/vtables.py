"""Catalog gcc-2.x (non-thunk) vtables in SCUS_974.81 .rodata/.data.

A vtable candidate: 8 zero bytes (entry 0), then >= MIN_SLOTS entries
{s16 delta; s16 index; u32 fn} where fn is 0, the pure-virtual stub, or inside .text.
For each vtable, also lists the code addresses that load its address (lui+addiu pairs),
which are the constructors/destructors that install it.

  python vtables.py [--min N] > analysis/vtables.tsv
"""
import collections
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, scan_refs, containing_func  # noqa: E402

PURE = 0x002BFF80
MIN_SLOTS = 4


def is_slot(img, a):
    delta, index = struct.unpack("<hh", img.bytes_at(a, 4))
    fn = img.word(a + 4)
    if index != 0 or not (-0x1000 < delta < 0x1000):
        return False
    return fn == 0 or fn == PURE or (img.text[0] <= fn < img.text[1] and fn % 4 == 0)


def find_vtables(img, min_slots):
    out = []
    for sec in (".rodata", ".data"):
        lo, hi = img.sections[sec]
        a = (lo + 7) & ~7
        while a + 8 * (min_slots + 1) <= hi:
            if img.bytes_at(a, 8) == b"\0" * 8:
                n, b = 0, a + 8
                while b + 8 <= hi and is_slot(img, b) and img.bytes_at(b, 8) != b"\0" * 8:
                    n += 1
                    b += 8
                real = sum(1 for i in range(n) if img.word(a + 8 + i * 8 + 4) not in (0,))
                if n >= min_slots and real >= min_slots:
                    out.append((a, n))
                    a = b
                    continue
            a += 8
    return out


def main():
    min_slots = int(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[1] == "--min" else MIN_SLOTS
    img = Image()
    vts = find_vtables(img, min_slots)
    vt_set = {a for a, _ in vts}
    users = collections.defaultdict(set)
    for site, kind, target in scan_refs(img, lambda x: x in vt_set):
        f = containing_func(img, site)
        users[target].add(f or site)
    print("vtable\tslots\tpure_slots\tinstalled_by\tslot_fns")
    for a, n in vts:
        fns = [img.word(a + 8 + i * 8 + 4) for i in range(n)]
        pure = sum(1 for f in fns if f == PURE)
        inst = ",".join(f"{u:08x}" for u in sorted(users[a]))
        print(f"{a:08x}\t{n + 1}\t{pure}\t{inst}\t" + ",".join(f"{f:x}" for f in fns))


if __name__ == "__main__":
    main()
