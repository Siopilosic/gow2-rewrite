"""Independent R5900 disassembler / xref scanner for SCUS_974.81 (cross-check for Ghidra).

Capstone (MIPS64 LE) decodes the MIPS III/IV base; R5900-only opcodes
(lq/sq, MMI, lqc2/sqc2, COP2) are decoded here by hand.  Absolute addresses
built with lui + (addiu|ori|lw|sw|...) are resolved by tracking the last lui per
register within a straight-line run.

Usage:
  python eedis.py dis <addr> [count|end]       disassemble (count instrs, or until jr ra + delay)
  python eedis.py xref <addr>                   every jal / lui-pair reference to addr (exact)
  python eedis.py xref-range <lo> <hi>          references to anything in [lo, hi)
  python eedis.py callers <func>                jal sites calling func
"""
import bisect
import csv
import os
import struct
import sys

import capstone

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ELF = os.path.join(ROOT, "extracted", "SCUS_974.81")

REGS = ["zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7",
        "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra"]


class Image:
    def __init__(self, path=ELF):
        d = open(path, "rb").read()
        self.raw = d
        e_phoff = struct.unpack_from("<I", d, 28)[0]
        _, off, vaddr, _, filesz, memsz, _, _ = struct.unpack_from("<8I", d, e_phoff)
        self.base, self.off, self.filesz, self.memsz = vaddr, off, filesz, memsz
        # .text bounds from section headers
        e_shoff, = struct.unpack_from("<I", d, 32)
        shnum, shstrndx = struct.unpack_from("<HH", d, 48)
        sh = [struct.unpack_from("<10I", d, e_shoff + i * 40) for i in range(shnum)]
        st = sh[shstrndx][4]
        self.sections = {}
        for s in sh:
            name = d[st + s[0]: d.index(b"\0", st + s[0])].decode()
            if s[3]:
                self.sections[name] = (s[3], s[3] + s[5])
        self.text = self.sections[".text"]
        self.names = load_names()
        self.md = capstone.Cs(capstone.CS_ARCH_MIPS, capstone.CS_MODE_MIPS64 + capstone.CS_MODE_LITTLE_ENDIAN)

    def word(self, a):
        return struct.unpack_from("<I", self.raw, self.off + a - self.base)[0]

    def bytes_at(self, a, n):
        o = self.off + a - self.base
        return self.raw[o:o + n]

    def in_file(self, a):
        return self.base <= a < self.base + self.filesz

    def cstr(self, a, maxlen=80):
        if not self.in_file(a):
            return None
        b = self.bytes_at(a, maxlen)
        end = b.find(b"\0")
        if end < 2:
            return None
        s = b[:end]
        if all(32 <= c < 127 or c in (9, 10) for c in s):
            return s.decode()
        return None

    def label(self, a):
        n = self.names.get(a)
        return n if n else f"0x{a:08x}"


def load_names():
    names = {}
    f = os.path.join(ROOT, "analysis", "exports", os.environ.get("GOW2_EXPORT", "pass1"), "functions.tsv")
    if os.path.exists(f):
        for r in csv.DictReader(open(f), delimiter="\t"):
            names[int(r["addr"], 16)] = r["name"]
    s = os.path.join(ROOT, "analysis", "symbols.tsv")
    if os.path.exists(s):
        for line in open(s):
            if line.strip() and not line.startswith("#"):
                c = line.rstrip("\n").split("\t")
                names[int(c[0], 16)] = c[2]
    return names


def r5900_special(w, a):
    """Decode R5900-specific encodings capstone can't handle. Returns (mnemonic, ops) or None."""
    op = w >> 26
    rs, rt, rd = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31
    imm = struct.unpack("<h", struct.pack("<H", w & 0xFFFF))[0]
    if op == 0x1E:
        return "lq", f"${REGS[rt]}, {imm}(${REGS[rs]})"
    if op == 0x1F:
        return "sq", f"${REGS[rt]}, {imm}(${REGS[rs]})"
    if op == 0x36:
        return "lqc2", f"$vf{rt}, {imm}(${REGS[rs]})"
    if op == 0x3E:
        return "sqc2", f"$vf{rt}, {imm}(${REGS[rs]})"
    if op == 0x1C:  # MMI
        fn = w & 0x3F
        names = {0x00: "madd", 0x01: "maddu", 0x04: "plzcw", 0x10: "mfhi1", 0x11: "mthi1", 0x12: "mflo1",
                 0x13: "mtlo1", 0x18: "mult1", 0x19: "multu1", 0x1A: "div1", 0x1B: "divu1", 0x20: "madd1",
                 0x21: "maddu1", 0x30: "pmfhl", 0x31: "pmthl", 0x34: "psllh", 0x36: "psrlh", 0x37: "psrah",
                 0x3C: "psllw", 0x3E: "psrlw", 0x3F: "psraw"}
        if fn in (0x08, 0x09, 0x28, 0x29):
            sub = (w >> 6) & 31
            tables = {
                0x08: ["paddw", "psubw", "pcgtw", "pmaxw", "paddh", "psubh", "pcgth", "pmaxh", "paddb", "psubb",
                       "pcgtb", None, None, None, None, None, "paddsw", "psubsw", "pextlw", "ppacw", "paddsh",
                       "psubsh", "pextlh", "ppach", "paddsb", "psubsb", "pextlb", "ppacb", None, None, "pext5",
                       "ppac5"],
                0x28: [None, "pabsw", "pceqw", "pminw", "padsbh", "pabsh", "pceqh", "pminh", None, None,
                       "pceqb", None, None, None, None, None, "padduw", "psubuw", "pextuw", None, "padduh",
                       "psubuh", "pextuh", None, "paddub", "psubub", "pextub", "qfsrv", None, None, None,
                       None],
                0x09: ["pmaddw", None, "psllvw", "psrlvw", "pmsubw", None, None, None, "pmfhi", "pmflo",
                       "pinth", None, "pmultw", "pdivw", "pcpyld", None, "pmaddh", "phmadh", "pand", "pxor",
                       "pmsubh", "phmsbh", None, None, None, None, "pexeh", "prevh", "pmulth", "pdivbw",
                       "pexew", "prot3w"],
                0x29: ["pmadduw", None, None, "psravw", None, None, None, None, "pmthi", "pmtlo", "pinteh",
                       None, "pmultuw", "pdivuw", "pcpyud", None, None, None, "por", "pnor", None, None, None,
                       None, None, None, "pexch", "pcpyh", None, None, "pexcw", None],
            }
            n = tables[fn][sub] if sub < 32 else None
            return (n or f"mmi{fn:x}.{sub}"), f"${REGS[rd]}, ${REGS[rs]}, ${REGS[rt]}"
        return names.get(fn, f"mmi.{fn:#x}"), f"${REGS[rd]}, ${REGS[rs]}, ${REGS[rt]}"
    if op == 0x12:
        return "cop2", f"0x{w & 0x1FFFFFF:07x}"
    if op == 0x01 and rt in (0x18, 0x19):
        return ("mtsab" if rt == 0x18 else "mtsah"), f"${REGS[rs]}, {imm}"
    if op == 0x00 and (w & 0x3F) == 0x28:
        return "mfsa", f"${REGS[rd]}"
    if op == 0x00 and (w & 0x3F) == 0x29:
        return "mtsa", f"${REGS[rs]}"
    if op == 0x00 and (w & 0x3F) in (0x18, 0x19) and rd:
        return ("mult" if (w & 0x3F) == 0x18 else "multu"), f"${REGS[rd]}, ${REGS[rs]}, ${REGS[rt]}"
    return None


def decode(img, a):
    w = img.word(a)
    sp = r5900_special(w, a)
    if sp:
        return w, sp[0], sp[1]
    insns = list(img.md.disasm(struct.pack("<I", w), a, 1))
    if not insns:
        return w, ".word", f"0x{w:08x}"
    i = insns[0]
    return w, i.mnemonic, i.op_str


MEMOPS = {"lb", "lbu", "lh", "lhu", "lw", "lwu", "ld", "sb", "sh", "sw", "sd", "lwc1", "swc1", "lq", "sq",
          "lqc2", "sqc2", "ldl", "ldr", "sdl", "sdr", "lwl", "lwr", "swl", "swr"}


def resolve_pairs(img, start, end):
    """Yield (addr, w, mnem, ops, resolved_abs or None, target_reg) over [start,end)."""
    hi = {}
    for a in range(start, end, 4):
        w, m, ops = decode(img, a)
        op = w >> 26
        rs, rt = (w >> 21) & 31, (w >> 16) & 31
        imm = w & 0xFFFF
        simm = imm - 0x10000 if imm & 0x8000 else imm
        res = None
        if op == 0x0F:  # lui
            hi[rt] = imm << 16
        elif rs in hi and (op in (0x09, 0x19) or m in MEMOPS):  # addiu/daddiu/mem
            res = (hi[rs] + simm) & 0xFFFFFFFF
            if op in (0x09, 0x19) and rt != rs:
                pass
            if op in (0x09, 0x19):
                hi.pop(rt, None) if rt != rs else hi.pop(rs, None)
        elif rs in hi and op == 0x0D:  # ori
            res = (hi[rs] | imm) & 0xFFFFFFFF
            hi.pop(rt, None)
        else:
            # any other write to rt/rd kills tracking for it
            if op not in (0x2B, 0x29, 0x28, 0x3F, 0x1F, 0x39, 0x3E) and op != 0x0F:
                hi.pop(rt, None)
                if op == 0:
                    hi.pop((w >> 11) & 31, None)
        if m in ("jr", "j", "jal", "b", "beq", "bne") or m.startswith("b"):
            pass
        yield a, w, m, ops, res


def fmt_line(img, a, w, m, ops, res):
    note = ""
    if m == "jal":
        t = ((a + 4) & 0xF0000000) | ((w & 0x3FFFFFF) << 2)
        ops = img.label(t)
    elif res is not None:
        s = img.cstr(res)
        note = f"  ; -> {img.label(res)}" + (f' "{s}"' if s else "")
    lbl = img.names.get(a)
    head = f"\n{lbl}:\n" if lbl and not lbl.startswith("0x") else ""
    return f"{head}  {a:08x}  {w:08x}  {m:<8} {ops}{note}"


def func_end(img, a, limit=20000):
    """Address after the first 'jr ra' + delay slot."""
    for x in range(a, a + limit * 4, 4):
        if img.word(x) == 0x03E00008:
            return x + 8
    return a + limit * 4


def cmd_dis(img, a, n=None):
    if n is None:
        end = func_end(img, a)
    elif n < 0x100000:
        end = a + n * 4
    else:
        end = n
    for row in resolve_pairs(img, a, end):
        print(fmt_line(img, *row))


def scan_refs(img, pred):
    lo, hi = img.text
    for a, w, m, ops, res in resolve_pairs(img, lo, hi):
        if m == "jal":
            t = ((a + 4) & 0xF0000000) | ((w & 0x3FFFFFF) << 2)
            if pred(t):
                yield a, "call", t
        elif res is not None and pred(res):
            yield a, m, res


def containing_func(img, a):
    if not hasattr(img, "_starts"):
        img._starts = sorted(k for k in img.names if img.text[0] <= k < img.text[1])
    i = bisect.bisect_right(img._starts, a) - 1
    return img._starts[i] if i >= 0 else None


def cmd_xref(img, lo, hi):
    for a, kind, t in scan_refs(img, lambda x: lo <= x < hi):
        f = containing_func(img, a)
        print(f"{a:08x}  {kind:<6} -> {img.label(t)}  (in {img.label(f) if f else '?'})")


if __name__ == "__main__":
    img = Image()
    cmd, args = sys.argv[1], [int(x, 16) if x.startswith("0x") or len(x) > 4 else int(x) for x in sys.argv[2:]]
    if cmd == "dis":
        cmd_dis(img, args[0], args[1] if len(args) > 1 else None)
    elif cmd == "xref":
        cmd_xref(img, args[0], args[0] + 1)
    elif cmd == "xref-range":
        cmd_xref(img, args[0], args[1])
    elif cmd == "callers":
        cmd_xref(img, args[0], args[0] + 1)
