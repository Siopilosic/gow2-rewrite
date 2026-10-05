"""Decode the move graph of a DC blob into tables, using only code-confirmed field offsets.

  python tools/dcmoves.py extracted/pak/R_HERO00.WAD [DC_WAD_R_Hero]

Writes analysis/dc/<WAD>/{moves,branches,actions}.tsv. Field evidence: docs/kratos-data.md (pass
"tag 0x10 decoding", 2026-10-02):
  MOV  +0x08 anim-clip name hash, +0x0c own name hash (tag 0x0f table)
       +0x10/+0x14/+0x18 packed lists (rel_off << 12 | count): branches / tCollision / actions
       (FUN_0024f250 reads +0x10, FUN_00247bd8 reads +0x18 and +0x14)
  tBranch (FUN_0024f250, FUN_0024e210, FUN_0024eb00, FUN_0024f1e8):
       +0x00 rel -> target MOV (data), +0x04 rel (compared with a context object),
       +0x08 u32 flags A (low bits = character-state mask; 0x200/0x400/0x800/0x1000/0x2000/0x4000),
       +0x0c u32 flags B, +0x10/+0x12 half window [start,end] in normalised anim time, +0x14 half start time in the
       target move (FUN_002470b8),
       +0x16/+0x18 s16 range on target +0x178, +0x1a/+0x1c s16 range on own +0x178,
       +0x1e s8 / +0x1f s8 target-class compares, +0x20 button code, +0x21 press mode,
       +0x22 stick condition, +0x23 unlock requirement, +0x24 s8 minimum level
  hit window (tCollision list entry; FUN_00247820, FUN_00247968):
       +0x00/+0x02 half window [start,end] in normalised move time, +0x04/+0x0a/+0x10 three halfs each:
       ground / air / block impulse (FUN_0024aa68, FUN_0024ad78), +0x16 half base damage (FUN_00249f38),
       +0x18 u8 attack volume selector,
       +0x19 u8 flags (1 = lock-on target only, 0x80 = skipped for the player in some modes),
       +0x1a s8 number of sub-hits spread over the window
  action (FUN_00247bd8): +0 kind, +1 flags, +2 trigger condition, +3 s8 minimum level,
       +4/+6 half window [start,end]
"""
import bisect
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import DC  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUTTON_BIT = {1: 6, 2: 7, 3: 4, 4: 5, 5: 2, 6: 0, 7: 9, 8: 3, 9: 1, 10: 10}  # table 0x002f6da7
PRESS = {1: "pressed", 2: "released", 3: "held", 4: "held", 5: "not_held"}


def half(h):
    return struct.unpack("<e", struct.pack("<H", h))[0]


def main():
    path = sys.argv[1]
    dc = DC(path, sys.argv[2] if len(sys.argv) > 2 else None)
    B = dc.blob
    objs = sorted(dc.objects)
    info = {o: (n, t) for o, n, t in objs}
    starts = [o for o, _, _ in objs]
    hname = {h: n for h, n in dc.hashes}
    imports = {o: n for o, n in dc.imports}
    wad = os.path.splitext(os.path.basename(path))[0]
    out = os.path.join(ROOT, "analysis", "dc", wad)
    os.makedirs(out, exist_ok=True)

    def u32(o):
        return struct.unpack_from("<I", B, o)[0]

    def rel(o):
        v = struct.unpack_from("<i", B, o)[0]
        return None if v == 0 else o + v

    def plist(o):
        w = u32(o)
        n = w & 0xFFF
        if not n:
            return []
        arr = o + (struct.unpack_from("<i", B, o)[0] >> 12)
        return [rel(arr + 4 * k) for k in range(n)]

    def name(o):
        if o is None:
            return ""
        if o in info:
            return info[o][0]
        i = bisect.bisect_right(starts, o) - 1
        return f"{objs[i][1]}+{o - objs[i][0]:#x}" if i >= 0 else f"blob+{o:#x}"

    mv = open(os.path.join(out, "moves.tsv"), "w", encoding="utf-8", newline="")
    br = open(os.path.join(out, "branches.tsv"), "w", encoding="utf-8", newline="")
    ac = open(os.path.join(out, "actions.tsv"), "w", encoding="utf-8", newline="")
    co = open(os.path.join(out, "collisions.tsv"), "w", encoding="utf-8", newline="")
    co.write("move\tidx\tcollision\twin_start\twin_end\tdamage\tground_impulse\tair_impulse\tblock_impulse\t"
             "volume\tflags\tsub_hits\traw\n")

    def vec3(o):
        return ",".join(f"{half(h):.4g}" for h in struct.unpack_from("<3H", B, o))

    mv.write("blob_off\tmove\tanim\tname_hash_ok\tn_branches\tn_collisions\tn_actions\tw00\tw04\n")
    br.write("move\tidx\tbranch\ttarget\tflagsA\tflagsB\twin_start\twin_end\ttgt178_min\ttgt178_max\t"
             "own178_min\town178_max\tb1e\tb1f\tbutton\tbutton_bit\tpress\tstick\tunlock_req\tmin_level\tstart_time\n")
    ac.write("move\tidx\taction\ttype\tkind\tflags\ttrigger\tmin_level\twin_start\twin_end\timport\traw\n")
    nm = nb = na = 0
    for o, n, t in objs:
        if t != 0x62:
            continue
        nm += 1
        brs, cols, acts = plist(o + 0x10), plist(o + 0x14), plist(o + 0x18)
        mv.write(f"{o:08x}\t{n}\t{hname.get(u32(o + 8), f'{u32(o + 8):08x}')}\t{int(hname.get(u32(o + 12)) == n)}\t"
                 f"{len(brs)}\t{len(cols)}\t{len(acts)}\t{u32(o):08x}\t{u32(o + 4):08x}\n")
        for k, c in enumerate(cols):
            if c is None:
                continue
            co.write(f"{n}\t{k}\t{name(c)}\t{half(struct.unpack_from('<H', B, c)[0]):.4g}\t"
                     f"{half(struct.unpack_from('<H', B, c + 2)[0]):.4g}\t"
                     f"{half(struct.unpack_from('<H', B, c + 0x16)[0]):.4g}\t"
                     f"{vec3(c + 4)}\t{vec3(c + 0xa)}\t{vec3(c + 0x10)}\t{B[c + 0x18]:#x}\t{B[c + 0x19]:#x}\t"
                     f"{struct.unpack_from('<b', B, c + 0x1a)[0]}\t{B[c:c + 0x20].hex()}\n")
        for k, b in enumerate(brs):
            nb += 1
            s16 = struct.unpack_from("<4h", B, b + 0x16)
            btn = B[b + 0x20]
            br.write(f"{n}\t{k}\t{name(b)}\t{name(rel(b))}\t{u32(b + 8):08x}\t{u32(b + 12):08x}\t"
                     f"{half(struct.unpack_from('<H', B, b + 0x10)[0]):.4g}\t"
                     f"{half(struct.unpack_from('<H', B, b + 0x12)[0]):.4g}\t"
                     f"{s16[0]}\t{s16[1]}\t{s16[2]}\t{s16[3]}\t"
                     f"{struct.unpack_from('<b', B, b + 0x1e)[0]}\t{struct.unpack_from('<b', B, b + 0x1f)[0]}\t"
                     f"{btn:#x}\t{BUTTON_BIT.get(btn, '')}\t{PRESS.get(B[b + 0x21], B[b + 0x21])}\t"
                     f"{B[b + 0x22]:#x}\t{B[b + 0x23]:#x}\t{struct.unpack_from('<b', B, b + 0x24)[0]}\t"
                     f"{half(struct.unpack_from('<H', B, b + 0x14)[0]):.4g}\n")
        for k, a in enumerate(acts):
            if a is None:
                continue
            na += 1
            imp = ",".join(imports[x] for x in range(a, a + 0x40, 4) if x in imports and
                           (bisect.bisect_right(starts, x) - 1) == (bisect.bisect_right(starts, a) - 1))
            raw = B[a:a + 16].hex()
            ac.write(f"{n}\t{k}\t{name(a)}\t{info.get(a, ('', -1))[1]:#x}\t{B[a]:#x}\t{B[a + 1]:#x}\t{B[a + 2]}\t"
                     f"{struct.unpack_from('<b', B, a + 3)[0]}\t"
                     f"{half(struct.unpack_from('<H', B, a + 4)[0]):.4g}\t"
                     f"{half(struct.unpack_from('<H', B, a + 6)[0]):.4g}\t{imp}\t{raw}\n")
    for f in (mv, br, ac):
        f.close()
    print(f"{wad}: {nm} moves, {nb} branch refs, {na} action refs -> {out}")


if __name__ == "__main__":
    main()
