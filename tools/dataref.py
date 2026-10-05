"""Find where function addresses are stored in the ELF image and show the surrounding words.

  python tools/dataref.py <addr> [...]

Each hit prints the address of the word and the 8 words around it; words that point to a C string
are shown as that string. Used to identify vtables, registration tables and callback tables.
"""
import struct
import sys
import os

ROOT = os.path.join(os.path.dirname(__file__), "..")
ELF = open(os.path.join(ROOT, "extracted", "SCUS_974.81"), "rb").read()
PH, N = struct.unpack_from("<I", ELF, 0x1c)[0], struct.unpack_from("<H", ELF, 0x2c)[0]
SEGS = [struct.unpack_from("<8I", ELF, PH + i * 32) for i in range(N)]


def off(va):
    for typ, o, v, _p, fs, _ms, _f, _a in SEGS:
        if typ == 1 and v <= va < v + fs:
            return o + va - v
    return None


def cstr(va):
    o = off(va)
    if o is None:
        return None
    end = ELF.find(b"\0", o, o + 64)
    s = ELF[o:end]
    if len(s) >= 3 and all(32 <= c < 127 for c in s):
        return s.decode()
    return None


def show(target):
    for typ, o, v, _p, fs, _ms, _f, _a in SEGS:
        if typ != 1:
            continue
        for i in range(0, fs - 3, 4):
            if struct.unpack_from("<I", ELF, o + i)[0] == target:
                va = v + i
                print(f"  word at {va:08x}:")
                for j in range(-4, 5):
                    w = struct.unpack_from("<I", ELF, o + i + 4 * j)[0]
                    s = cstr(w)
                    print(f"    {va + 4 * j:08x}  {w:08x}{'  ' + repr(s) if s else ''}{'  <--' if j == 0 else ''}")


def code_refs(target):
    """Instructions that build `target` with lui + addiu/ori (same base register, any distance)."""
    hits = []
    for typ, o, v, _p, fs, _ms, _f, _a in SEGS:
        if typ != 1:
            continue
        hi = {}
        for i in range(0, fs - 3, 4):
            w = struct.unpack_from("<I", ELF, o + i)[0]
            op = w >> 26
            if op == 0x0F:
                hi[(w >> 16) & 31] = (w & 0xFFFF) << 16
            elif op in (0x09, 0x0D) and ((w >> 21) & 31) in hi:
                base, lo = hi[(w >> 21) & 31], w & 0xFFFF
                val = base | lo if op == 0x0D else base + (lo - 0x10000 if lo & 0x8000 else lo)
                if val == target:
                    hits.append(v + i)
    return hits


if __name__ == "__main__":
    for a in sys.argv[1:]:
        print(a, "code refs:", " ".join(f"{h:08x}" for h in code_refs(int(a, 16))))


if __name__ == "__main__":
    for a in sys.argv[1:]:
        print(a)
        show(int(a, 16))
