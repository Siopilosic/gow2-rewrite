"""Collect concrete compiler/ABI evidence from SCUS_974.81 (results -> docs/confirmed.md).

Prints:
  * compiler-identifying strings
  * $gp-relative access count and the gp value set by crt0
  * COP1 instruction format histogram (single vs double precision)
  * __CTOR_LIST__ contents (0x002de0ac) and the __terminate_func pointer (0x002db8f0)
  * argument-register usage of jal call sites (how many set t0..t3 / f12..f19 before a call)
  * stack-frame alignment (addiu sp,sp,-N values mod 16)
"""
import collections
import os
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, decode  # noqa: E402


def main():
    img = Image()
    raw = img.raw
    print("== compiler strings")
    for pat in (rb"GCC[ :][^\0]{0,40}", rb"gcc[^\0]{0,30}", rb"ProDG[^\0]{0,30}", rb"SN Sys[^\0]{0,30}",
                rb"__pure_virtual", rb"__vt_[^\0]{0,30}", rb"pure virtual[^\0]{0,30}"):
        hits = {m.group() for m in re.finditer(pat, raw)}
        print(f"  {pat.decode(errors='replace'):<24} {sorted(hits)[:5] if hits else 'none'}")

    lo, hi = img.text
    gp_uses = 0
    fmt = collections.Counter()
    frames = collections.Counter()
    for a in range(lo, hi, 4):
        w = img.word(a)
        op, rs = w >> 26, (w >> 21) & 31
        if op in (0x23, 0x2B, 0x09, 0x31, 0x39, 0x21, 0x25, 0x29, 0x20, 0x24, 0x28) and rs == 28:
            gp_uses += 1
        if op == 0x11:  # COP1
            f = rs
            fmt[{0x10: "S(single)", 0x11: "D(double)", 0x14: "W(word)", 0x15: "L(long)", 0x00: "mfc1",
                 0x04: "mtc1", 0x02: "cfc1", 0x06: "ctc1", 0x08: "bc1"}.get(f, f"fmt{f:#x}")] += 1
        if (w & 0xFFFF0000) == 0x27BD0000 and w & 0x8000:  # addiu sp,sp,-N
            n = 0x10000 - (w & 0xFFFF)
            frames["mod16=0" if n % 16 == 0 else f"mod16={n % 16}"] += 1
    print(f"== $gp-relative loads/stores/addiu: {gp_uses}")
    print(f"== COP1 formats: {dict(fmt)}")
    print(f"== stack frame sizes: {dict(frames)}")

    print("== __CTOR_LIST__ @ 0x002de0ac")
    a = 0x002DE0AC
    for i in range(12):
        w = img.word(a + i * 4)
        print(f"  [{i}] {w:08x}")
        if i and w == 0:
            break
    print(f"== __terminate_func ptr @ 0x002db8f0 = {img.word(0x002DB8F0):08x}")

    # call-site argument registers: look back 6 instructions before each jal
    regs_set = collections.Counter()
    calls = 0
    for a in range(lo, hi, 4):
        if img.word(a) >> 26 != 0x03:
            continue
        calls += 1
        window = [a - 4 * k for k in range(1, 7)] + [a + 4]
        seen = set()
        for x in window:
            w = img.word(x)
            op = w >> 26
            rt, rd = (w >> 16) & 31, (w >> 11) & 31
            dst = rd if op == 0 else rt
            if op in (0x09, 0x0D, 0x0F, 0x23, 0x00, 0x19) and 8 <= dst <= 11:
                seen.add(f"t{dst - 8}")
            if op == 0x11 and ((w >> 21) & 31) in (0x10, 0x04):
                fd = (w >> 6) & 31 if ((w >> 21) & 31) == 0x10 else (w >> 11) & 31
                if 12 <= fd <= 19:
                    seen.add(f"f{fd}")
        for r in seen:
            regs_set[r] += 1
    print(f"== jal sites: {calls}; arg registers set within 6 insns before call / in delay slot:")
    print("  " + ", ".join(f"{k}:{v}" for k, v in sorted(regs_set.items())))


if __name__ == "__main__":
    main()
