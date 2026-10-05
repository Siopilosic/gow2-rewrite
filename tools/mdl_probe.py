"""Walk the MDL_ subtype-1 blob hierarchy and decode DMA tags / VIF codes. Read-only probe.

  python tools/mdl_probe.py extracted/pak/R_HERO00.WAD MDL_hero_0 [max_packets]

Hierarchy from the relocation routine FUN_00168168 (docs/models.md):
  blob: u16 +0x08 nA; u32 off[nA] at +0x18 (rel. blob)
  A:    u16 +0x02 nB; u32 off[nB] at +0x04 (rel. A)
  B:    u16 +0x04 nC; u32 off[nC] at +0x08 (rel. B)
  C:    s16 +0x00 kind; kinds 0x18/0x0e: groups = u8 +0x18 * u32 +0x0c, per group u32 +0x04
        16-byte entries from +0x20 whose word +4 is relocated by C (an address).
Everything else printed here (DMA/VIF meaning) is the standard PS2 encoding being *tested*.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402

DMA_ID = {0: "refe", 1: "cnt", 2: "next", 3: "ref", 4: "refs", 5: "call", 6: "ret", 7: "end"}
VIF_CMD = {0x00: "NOP", 0x01: "STCYCL", 0x02: "OFFSET", 0x03: "BASE", 0x04: "ITOP", 0x05: "STMOD",
           0x06: "MSKPATH3", 0x07: "MARK", 0x10: "FLUSHE", 0x11: "FLUSH", 0x13: "FLUSHA", 0x14: "MSCAL",
           0x15: "MSCALF", 0x17: "MSCNT", 0x20: "STMASK", 0x30: "STROW", 0x31: "STCOL", 0x4A: "MPG",
           0x50: "DIRECT", 0x51: "DIRECTHL"}
UNPACK = {0x0: "S-32", 0x1: "S-16", 0x2: "S-8", 0x4: "V2-32", 0x5: "V2-16", 0x6: "V2-8", 0x8: "V3-32",
          0x9: "V3-16", 0xA: "V3-8", 0xC: "V4-32", 0xD: "V4-16", 0xE: "V4-8", 0xF: "V4-5"}
UNPACK_SIZE = {0x0: 4, 0x1: 2, 0x2: 1, 0x4: 8, 0x5: 4, 0x6: 2, 0x8: 12, 0x9: 6, 0xA: 3, 0xC: 16, 0xD: 8,
               0xE: 4, 0xF: 2}


def vif_walk(buf, pos, end, out, limit=40):
    n = 0
    while pos + 4 <= end and n < limit:
        code = struct.unpack_from("<I", buf, pos)[0]
        cmd, num, imm = (code >> 24) & 0x7F, (code >> 16) & 0xFF, code & 0xFFFF
        pos += 4
        if cmd >= 0x60:
            vn, vl = (cmd >> 2) & 3, cmd & 3
            fmt = (vn << 2) | vl
            size = UNPACK_SIZE.get(fmt, 0) * (num or 256)
            size = (size + 3) & ~3
            out.append(f"      UNPACK {UNPACK.get(fmt, hex(fmt))} num={num} addr={imm & 0x3ff:#x} "
                       f"{'usn ' if imm & 0x4000 else ''}{'+tops ' if imm & 0x8000 else ''}{'mask' if cmd & 0x10 else ''}")
            pos += size
        else:
            name = VIF_CMD.get(cmd, f"?{cmd:#x}")
            out.append(f"      {name} num={num} imm={imm:#x}")
            if cmd in (0x20,):
                pos += 4
            elif cmd in (0x30, 0x31):
                pos += 16
            elif cmd == 0x4A:
                pos += (num or 256) * 8
            elif cmd in (0x50, 0x51):
                pos += (imm or 65536) * 16
        n += 1
    return pos


def main():
    path, name = sys.argv[1], sys.argv[2]
    limit = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    data = open(path, "rb").read()
    blob = next(body for off, tag, param, nm, body in wad_records(data) if tag == 1 and nm == name)
    u16 = lambda o: struct.unpack_from("<H", blob, o)[0]  # noqa: E731
    u32 = lambda o: struct.unpack_from("<I", blob, o)[0]  # noqa: E731
    print(f"{name}: {len(blob)} bytes; header words: {[hex(u32(k)) for k in range(0, 0x18, 4)]}")
    nA = u16(8)
    shown = 0
    for i in range(nA):
        A = u32(0x18 + 4 * i)
        nB = u16(A + 2)
        print(f" A[{i}] @{A:#x} nB={nB} head={[hex(u32(A + k)) for k in (0,)]}")
        for j in range(nB):
            Bo = A + u32(A + 4 + 4 * j)
            nC = u16(Bo + 4)
            print(f"  B[{j}] @{Bo:#x} nC={nC} head={blob[Bo:Bo + 8].hex()}")
            for k in range(nC):
                C = Bo + u32(Bo + 8 + 4 * k)
                kind = struct.unpack_from("<h", blob, C)[0]
                print(f"   C[{k}] @{C:#x} kind={kind:#x} words={[hex(u32(C + q)) for q in range(0, 0x20, 4)]}")
                if kind in (0x18, 0x0E) and shown < limit:
                    shown += 1
                    groups, per = blob[C + 0x18] * u32(C + 0xC), u32(C + 4)
                    e = C + 0x20
                    for g in range(min(groups, 2)):
                        for q in range(per):
                            w0, addr, v0, v1 = struct.unpack_from("<4I", blob, e)
                            qwc, tid = w0 & 0xFFFF, (w0 >> 28) & 7
                            tgt = C + addr
                            print(f"    g{g} e{q}: DMA {DMA_ID[tid]} qwc={qwc} addr=C+{addr:#x}(blob {tgt:#x}) "
                                  f"vif0={v0:08x} vif1={v1:08x}")
                            out = []
                            if tid in (0, 3, 4) and 0 <= tgt < len(blob):
                                vif_walk(blob, tgt, tgt + qwc * 16, out, 12)
                            print("\n".join(out))
                            e += 16


if __name__ == "__main__":
    main()
