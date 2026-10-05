"""VU micro-mode disassembler for the ELF's .DVP.overlay sections (research tool).

  python tools/vu_disasm.py extracted/SCUS_974.81 --list
  python tools/vu_disasm.py extracted/SCUS_974.81 --prog 56370531 [--out file.txt]

Encoding from the public VU instruction set (Sony VU User's Manual / ps2tek): each instruction is
64 bits, upper word at +4 (FMAC) and lower word at +0 (integer/load/branch/EFU). If the upper I
bit (31) is set, the lower word is a float immediate for the I register. Sections of one program
share the hash in their name (.DVP.overlay..<vma|unknvma>.<hash>.<line>.<n>) and are concatenated in
index order; the VU address of index 0 is <vma> (0 if 'unknvma' is only used for continuation).
"""
import argparse
import collections
import re
import struct

BC = "xyzw"
UPPER = {}
for i, n in enumerate(["ADD", "SUB", "MADD", "MSUB", "MAX", "MINI", "MUL"]):
    for b in range(4):
        UPPER[i * 4 + b] = (n, BC[b])
UPPER.update({0x1C: ("MUL", "q"), 0x1D: ("MAX", "i"), 0x1E: ("MUL", "i"), 0x1F: ("MINI", "i"),
              0x20: ("ADD", "q"), 0x21: ("MADD", "q"), 0x22: ("ADD", "i"), 0x23: ("MADD", "i"),
              0x24: ("SUB", "q"), 0x25: ("MSUB", "q"), 0x26: ("SUB", "i"), 0x27: ("MSUB", "i"),
              0x28: ("ADD", ""), 0x29: ("MADD", ""), 0x2A: ("MUL", ""), 0x2B: ("MAX", ""),
              0x2C: ("SUB", ""), 0x2D: ("MSUB", ""), 0x2E: ("OPMSUB", ""), 0x2F: ("MINI", "")})
USPEC = {}
for i, n in enumerate(["ADDA", "SUBA", "MADDA", "MSUBA"]):
    for b in range(4):
        USPEC[i * 4 + b] = (n, BC[b])
for b in range(4):
    USPEC[0x18 + b] = ("MULA", BC[b])
USPEC.update({0x10: ("ITOF0", None), 0x11: ("ITOF4", None), 0x12: ("ITOF12", None), 0x13: ("ITOF15", None),
              0x14: ("FTOI0", None), 0x15: ("FTOI4", None), 0x16: ("FTOI12", None), 0x17: ("FTOI15", None),
              0x1C: ("MULA", "q"), 0x1D: ("ABS", None), 0x1E: ("MULA", "i"), 0x1F: ("CLIP", "w"),
              0x20: ("ADDA", "q"), 0x21: ("MADDA", "q"), 0x22: ("ADDA", "i"), 0x23: ("MADDA", "i"),
              0x24: ("SUBA", "q"), 0x25: ("MSUBA", "q"), 0x26: ("SUBA", "i"), 0x27: ("MSUBA", "i"),
              0x28: ("ADDA", ""), 0x29: ("MADDA", ""), 0x2A: ("MULA", ""), 0x2C: ("SUBA", ""),
              0x2D: ("MSUBA", ""), 0x2E: ("OPMULA", ""), 0x2F: ("NOP", None)})
LMAIN = {0x00: "LQ", 0x01: "SQ", 0x04: "ILW", 0x05: "ISW", 0x08: "IADDIU", 0x09: "ISUBIU",
         0x10: "FCEQ", 0x11: "FCSET", 0x12: "FCAND", 0x13: "FCOR", 0x14: "FSEQ", 0x15: "FSSET",
         0x16: "FSAND", 0x17: "FSOR", 0x18: "FMEQ", 0x1A: "FMAND", 0x1B: "FMOR", 0x1C: "FCGET",
         0x20: "B", 0x21: "BAL", 0x24: "JR", 0x25: "JALR", 0x28: "IBEQ", 0x29: "IBNE",
         0x2C: "IBLTZ", 0x2D: "IBGTZ", 0x2E: "IBLEZ", 0x2F: "IBGEZ"}
LOP = {0x30: "IADD", 0x31: "ISUB", 0x32: "IADDI", 0x34: "IAND", 0x35: "IOR"}
LSPEC = {0x30: "MOVE", 0x31: "MR32", 0x34: "LQI", 0x35: "SQI", 0x36: "LQD", 0x37: "SQD", 0x38: "DIV",
         0x39: "SQRT", 0x3A: "RSQRT", 0x3B: "WAITQ", 0x3C: "MTIR", 0x3D: "MFIR", 0x3E: "ILWR",
         0x3F: "ISWR", 0x40: "RNEXT", 0x41: "RGET", 0x42: "RINIT", 0x43: "RXOR", 0x64: "MFP",
         0x68: "XTOP", 0x69: "XITOP", 0x6C: "XGKICK", 0x70: "ESADD", 0x71: "ERSADD", 0x72: "ELENG",
         0x73: "ERLENG", 0x74: "EATANxy", 0x75: "EATANxz", 0x76: "ESUM", 0x78: "ESQRT", 0x79: "ERSQRT",
         0x7A: "ERCPR", 0x7B: "WAITP", 0x7C: "ESIN", 0x7D: "EATAN", 0x7E: "EEXP"}


def dest(w):
    return "".join(c for c, bit in zip("xyzw", (24, 23, 22, 21)) if w >> bit & 1)


def fsf(w):
    return "xyzw"[w >> 21 & 3]


def ftf(w):
    return "xyzw"[w >> 23 & 3]


def upper(w):
    op = w & 0x3F
    d, ft, fs, fd = dest(w), w >> 16 & 31, w >> 11 & 31, w >> 6 & 31
    flags = "".join(f for f, b in (("[I]", 31), ("[E]", 30), ("[M]", 29), ("[D]", 28), ("[T]", 27)) if w >> b & 1)
    if op >= 0x3C:
        sp = (w >> 6 & 31) << 2 | (op & 3)
        n, bc = USPEC.get(sp, (f"?u{sp:x}", None))
        if n == "NOP":
            s = "nop"
        elif n.startswith(("ITOF", "FTOI")) or n == "ABS":
            s = f"{n.lower()}.{d} vf{ft:02d}, vf{fs:02d}"
        elif n == "CLIP":
            s = f"clipw.xyz vf{fs:02d}, vf{ft:02d}w"
        elif n == "OPMULA":
            s = f"opmula.xyz ACC, vf{fs:02d}, vf{ft:02d}"
        elif bc in ("q", "i"):
            s = f"{n.lower()}{bc}.{d} ACC, vf{fs:02d}, {bc.upper()}"
        elif bc:
            s = f"{n.lower()}{bc}.{d} ACC, vf{fs:02d}, vf{ft:02d}{bc}"
        else:
            s = f"{n.lower()}.{d} ACC, vf{fs:02d}, vf{ft:02d}"
    else:
        n, bc = UPPER.get(op, (f"?U{op:x}", ""))
        if n == "OPMSUB":
            s = f"opmsub.xyz vf{fd:02d}, vf{fs:02d}, vf{ft:02d}"
        elif bc in ("q", "i"):
            s = f"{n.lower()}{bc}.{d} vf{fd:02d}, vf{fs:02d}, {bc.upper()}"
        elif bc:
            s = f"{n.lower()}{bc}.{d} vf{fd:02d}, vf{fs:02d}, vf{ft:02d}{bc}"
        else:
            s = f"{n.lower()}.{d} vf{fd:02d}, vf{fs:02d}, vf{ft:02d}"
    return s + (" " + flags if flags else "")


def s11(v):
    v &= 0x7FF
    return v - 0x800 if v & 0x400 else v


def lower(w, pc):
    op = w >> 25
    d, it, is_, id_ = dest(w), w >> 16 & 31, w >> 11 & 31, w >> 6 & 31
    if op == 0x40:
        sub = w & 0x3F
        if sub < 0x3C:
            n = LOP.get(sub, f"?l{sub:x}")
            if n == "IADDI":
                imm5 = (w >> 6 & 31) - (32 if w >> 10 & 1 else 0)
                return f"iaddi vi{it:02d}, vi{is_:02d}, {imm5}"
            return f"{n.lower()} vi{id_:02d}, vi{is_:02d}, vi{it:02d}"
        sp = (w >> 6 & 31) << 2 | (sub & 3)
        n = LSPEC.get(sp, f"?L{sp:x}")
        if n in ("MOVE", "MR32"):
            return f"{n.lower()}.{d} vf{it:02d}, vf{is_:02d}"
        if n in ("LQI", "LQD"):
            return f"{n.lower()}.{d} vf{it:02d}, (vi{is_:02d}{'++' if n == 'LQI' else '--'})"
        if n in ("SQI", "SQD"):
            return f"{n.lower()}.{d} vf{is_:02d}, (vi{it:02d}{'++' if n == 'SQI' else '--'})"
        if n in ("DIV", "RSQRT"):
            return f"{n.lower()} Q, vf{is_:02d}{fsf(w)}, vf{it:02d}{ftf(w)}"
        if n == "SQRT":
            return f"sqrt Q, vf{it:02d}{ftf(w)}"
        if n == "WAITQ" or n == "WAITP":
            return n.lower()
        if n == "MTIR":
            return f"mtir vi{it:02d}, vf{is_:02d}{fsf(w)}"
        if n == "MFIR":
            return f"mfir.{d} vf{it:02d}, vi{is_:02d}"
        if n in ("ILWR", "ISWR"):
            return f"{n.lower()}.{d} vi{it:02d}, (vi{is_:02d})"
        if n == "XGKICK":
            return f"xgkick vi{is_:02d}"
        if n in ("XTOP", "XITOP"):
            return f"{n.lower()} vi{it:02d}"
        if n == "MFP":
            return f"mfp.{d} vf{it:02d}, P"
        if n.startswith("E") and n not in ("ELENG", "ERLENG", "ESADD", "ERSADD", "ESUM"):
            return f"{n.lower()} P, vf{is_:02d}{fsf(w)}"
        if n.startswith("E"):
            return f"{n.lower()} P, vf{is_:02d}"
        if n in ("RNEXT", "RGET"):
            return f"{n.lower()}.{d} vf{it:02d}, R"
        if n in ("RINIT", "RXOR"):
            return f"{n.lower()} R, vf{is_:02d}{fsf(w)}"
        return n.lower()
    n = LMAIN.get(op, f"?M{op:x}")
    imm11 = s11(w)
    if n in ("LQ", "SQ"):
        return (f"lq.{d} vf{it:02d}, {imm11}(vi{is_:02d})" if n == "LQ"
                else f"sq.{d} vf{is_:02d}, {imm11}(vi{it:02d})")
    if n in ("ILW", "ISW"):
        return f"{n.lower()}.{d} vi{it:02d}, {imm11}(vi{is_:02d})"
    if n in ("IADDIU", "ISUBIU"):
        imm15 = (w >> 10 & 0x7800) | (w & 0x7FF)
        return f"{n.lower()} vi{it:02d}, vi{is_:02d}, 0x{imm15:x}"
    if n in ("B", "BAL"):
        t = pc + 1 + imm11
        return f"b 0x{t * 8:04x}" if n == "B" else f"bal vi{it:02d}, 0x{t * 8:04x}"
    if n in ("JR", "JALR"):
        return f"jr vi{is_:02d}" if n == "JR" else f"jalr vi{it:02d}, vi{is_:02d}"
    if n.startswith("IB"):
        t = pc + 1 + imm11
        if n in ("IBEQ", "IBNE"):
            return f"{n.lower()} vi{it:02d}, vi{is_:02d}, 0x{t * 8:04x}"
        return f"{n.lower()} vi{is_:02d}, 0x{t * 8:04x}"
    if n.startswith(("FC", "FS", "FM")):
        if n in ("FMEQ", "FMAND", "FMOR"):
            return f"{n.lower()} vi{it:02d}, vi{is_:02d}"
        if n == "FCGET":
            return f"fcget vi{it:02d}"
        imm = w & 0xFFFFFF if n.startswith("FC") else ((w >> 10 & 0x800) | (w & 0x7FF))
        return f"{n.lower()} {'vi01, ' if n not in ('FCSET', 'FSSET') else ''}0x{imm:x}"
    return n


def disasm(code, base=0):
    out = []
    for k in range(len(code) // 8):
        lo, hi = struct.unpack_from("<II", code, k * 8)
        pc = base // 8 + k
        u = upper(hi)
        if hi >> 31 & 1:
            ls = f"loi {struct.unpack('<f', struct.pack('<I', lo))[0]:g}"
        else:
            ls = lower(lo, pc)
        out.append(f"{pc * 8:04x}: {u:44s} {ls}")
    return out


def overlays(elf):
    d = open(elf, "rb").read()
    shoff, = struct.unpack_from("<I", d, 0x20)
    n, si = struct.unpack_from("<HH", d, 0x30)
    sh = [struct.unpack_from("<10I", d, shoff + i * 40) for i in range(n)]
    st = sh[si][4]
    progs = collections.OrderedDict()
    for s in sh:
        nm = d[st + s[0]:d.index(b"\0", st + s[0])].decode()
        m = re.match(r"\.DVP\.overlay\.\.(0x[0-9a-f]+|unknvma)\.(\d+)\.(\d+)\.(\d+)$", nm)
        if m:
            progs.setdefault(m.group(2), []).append((int(m.group(4)), m.group(1), d[s[4]:s[4] + s[5]], s[4]))
    return progs


DATA_FO, DATA_VA, DATA_SZ = 0x1D8780, 0x2D7780, 0x11780 + 0x12EA8 + 0x7B0  # .data .. end of .rodata


def mpg_blocks(elf):
    """The .DVP.overlay sections are zero-filled in this ELF; the microcode itself is in .data as
    DMA-chained VIF MPG packets (cmd 0x4a, num, vuaddr/8) preceded by a VIF NOP word (0).
    Returns [(ee_va, vu_addr, code)] in file order (docs/rendering.md)."""
    d = open(elf, "rb").read()
    out = []
    for o in range(DATA_FO, DATA_FO + DATA_SZ - 8, 4):
        w, = struct.unpack_from("<I", d, o)
        if (w >> 24) & 0x7F != 0x4A or struct.unpack_from("<I", d, o - 4)[0] != 0 or (o + 4) % 8:
            continue
        n = (w >> 16 & 0xFF) or 256
        code = d[o + 4:o + 4 + n * 8]
        nops = sum(1 for k in range(0, len(code), 8)
                   if code[k + 4:k + 8] == b"\xff\x02\0\0" or code[k:k + 4] == b"\x3c\x03\0\x80")
        if nops >= 3:
            out.append((o - DATA_FO + DATA_VA, (w & 0xFFFF) * 8, code))
    return out


def data_programs(elf):
    """Group consecutive MPG blocks into programs: a new program starts at VU address 0 or when a
    block does not continue the previous one's end (VU0 helpers at 0x400/0x200 kept separate)."""
    progs, cur, end = [], None, None
    for va, vu, code in mpg_blocks(elf):
        if cur is None or vu != end:
            cur = {"ee": va, "blocks": []}
            progs.append(cur)
        cur["blocks"].append((va, vu, code))
        end = vu + len(code)
    return progs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("elf")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--prog")
    ap.add_argument("--data", action="store_true", help="disassemble the MPG programs found in .data")
    ap.add_argument("--outdir")
    ap.add_argument("--out")
    a = ap.parse_args()
    if a.data:
        for p in data_programs(a.elf):
            size = sum(len(c) for _v, _u, c in p["blocks"])
            print(f"program @EE 0x{p['ee']:08x}: {len(p['blocks'])} blocks, 0x{size:x} bytes, "
                  f"VU 0x{p['blocks'][0][1]:04x}..0x{p['blocks'][-1][1] + len(p['blocks'][-1][2]):04x}")
            if a.outdir:
                lines = []
                for va, vu, code in p["blocks"]:
                    lines.append(f"; ---- MPG @EE 0x{va:08x} -> VU 0x{vu:04x} (0x{len(code):x} bytes)")
                    lines += disasm(code, vu)
                open(f"{a.outdir}/vu_{p['ee']:08x}.txt", "w").write("\n".join(lines) + "\n")
        return
    progs = overlays(a.elf)
    if a.list or not a.prog:
        for h, parts in progs.items():
            parts.sort()
            print(h, [(i, v, hex(len(c)), hex(off)) for i, v, c, off in parts])
        return
    parts = sorted(progs[a.prog])
    lines = []
    addr = int(parts[0][1], 16) if parts[0][1] != "unknvma" else 0
    for i, v, c, off in parts:
        if v != "unknvma":
            addr = int(v, 16) * 8 if int(v, 16) < 0x400 and False else int(v, 16)
        lines.append(f"; ---- part {i} vma {v} file 0x{off:x} size 0x{len(c):x}")
        lines += disasm(c, addr)
        addr += len(c)
    txt = "\n".join(lines)
    if a.out:
        open(a.out, "w").write(txt + "\n")
    else:
        print(txt)


if __name__ == "__main__":
    main()
