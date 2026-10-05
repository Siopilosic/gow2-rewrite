"""Find function starts that Ghidra's auto-analysis missed.

  python tools/missed_functions.py [--pass pass7]   (writes analysis/missed_functions.tsv)

A candidate is an address in game code that the program builds with lui + addiu/ori (a callback
passed by pointer, e.g. a script native), that is not a known function start, and whose word two
instructions earlier is `jr ra` (the previous function's return; the next word is its delay slot),
or three instructions earlier when GCC padded the gap with one nop for 8-byte alignment.
ApplyCore.java creates functions at these addresses on the next Ghidra pass.
"""
import argparse
import bisect
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
import dataref as D  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), "..")
LIB_START = 0x2A7748


def after_return(po):
    """True when the word at file offset `po` follows `jr ra` + delay slot, optionally + one nop."""
    w = lambda k: struct.unpack_from("<I", D.ELF, po - 4 * k)[0]
    return w(2) == 0x03E00008 or (w(3) == 0x03E00008 and w(1) == 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pass", dest="pass_", default="pass7")
    a = ap.parse_args()
    funcs = []
    for line in open(os.path.join(ROOT, "analysis", "exports", a.pass_, "functions.tsv"), encoding="utf-8"):
        if line.startswith(("#", "addr")):
            continue
        f = line.split("\t")
        funcs.append((int(f[0], 16), int(f[2])))
    funcs.sort()
    starts = {s for s, _ in funcs}
    keys = [s for s, _ in funcs]
    found = {}
    for typ, o, _v, _p, fs, _m, _f, _a in D.SEGS:
        if typ != 1:
            continue
        hi = {}
        for i in range(0, fs - 3, 4):
            w = struct.unpack_from("<I", D.ELF, o + i)[0]
            op = w >> 26
            if op == 0x0F:
                hi[(w >> 16) & 31] = (w & 0xFFFF) << 16
            elif op in (0x09, 0x0D) and ((w >> 21) & 31) in hi:
                base, lo = hi[(w >> 21) & 31], w & 0xFFFF
                val = base | lo if op == 0x0D else base + (lo - 0x10000 if lo & 0x8000 else lo)
                if 0x100000 <= val < LIB_START and val % 4 == 0 and val not in starts:
                    po = D.off(val)
                    if po and after_return(po):
                        k = bisect.bisect_right(keys, val) - 1
                        found[val] = int(keys[k] + funcs[k][1] > val)
    out = os.path.join(ROOT, "analysis", "missed_functions.tsv")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("#addr\tinside_existing_function\n")
        for v, inside in sorted(found.items()):
            fh.write(f"{v:08x}\t{inside}\n")
    print(len(found), "missed function starts;", sum(found.values()), "inside an existing function body")


if __name__ == "__main__":
    main()
