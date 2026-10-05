"""Decode ANM_ clip curves (docs/animation.md). Research tool.

Codec as read from the transform sampler FUN_00112380 (segment path) and helpers:
  clip block k at clip+0x60+16k: u16 ?, u16 segCount, u16 ?, u16 ?, u32 segTable, f32 dt
  segment (12 B): u16 slot, u16 flags, u16 keyCount, u16 start, u16 extra|offHi<<14, u16 offLo
     data = seg + (offHi<<16 | offLo) + (flags>>8)*0x10000
  keyCount == 0: data[0] = number of int8-delta runs, data[1] = total runs; runs (4 u16):
     count, startFrame, extra|offHi, offLo  -> run data at seg + table.base + (offHi<<16|offLo)
  shift table: flags & 2 ? data + (data[1]<<3 | 2) : runtime default {rows 1, stride 1, base 0, mask 1}
     rows, stride, u16 base, u16 mask[rows], int8 shift[...] (stride 1: shift = (s8)flags >> 4)
  accumulators are 2^14 fixed point; slot = seg.slot + 16*row + bit index in mask.
  absolute runs set the value; delta runs add delta * 2^-shift per frame (see KINDS for types).

  python tools/anm_decode.py extracted/pak/RHOD10.WAD ANM_swinginlamp
"""
import struct
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402

DEFAULT_TABLE = bytes([1, 1, 0, 0, 1, 0, 0, 0])


def u16(b, o):
    return struct.unpack_from("<H", b, o)[0]


def table_of(b, seg, data, keyed):
    flags = u16(b, seg + 2)
    if flags & 2:
        # keyed segments keep the table at the data offset; run lists put it after the runs
        return b, (data if keyed else data + ((b[data + 1] << 3) | 2))
    return DEFAULT_TABLE, 0


# per block kind: (delta format, absolute format, delta scale, absolute scale)
#   rot   (block 0): s8 deltas, s16 keys, int units (quaternion, 16384 = 1.0)  FUN_00112380/FUN_0010ab90
#   trans (block 1): s16 deltas x 1/256, f32 keys                              FUN_00272730/FUN_00272b28
#   scale (block 2): u16 keys x 1/2048 (no delta path seen)                    FUN_00272430
KINDS = {"rot": ("b", "h", 1.0, 1.0), "trans": ("h", "f", 1 / 256, 1.0), "scale": ("h", "H", 1 / 2048, 1 / 2048)}


def decode_segment(b, seg, kind="rot", acc=None):
    """Decode one segment. `acc` (slot -> value) is shared by all segments of a clip block: the game keeps
    one accumulator per slot (clip instance +0x5c), so a delta run continues from a key set by another
    segment. Pass the same dict for every segment of a block; default is a fresh one."""
    dfmt, afmt, dsc, asc = KINDS[kind]
    dsz, asz = struct.calcsize(dfmt), struct.calcsize(afmt)
    slot, flags, nkeys, start, ex, lo = struct.unpack_from("<6H", b, seg)
    data = seg + (((ex & 0xC000) << 2) | lo) + (flags >> 8) * 0x10000
    tb, t = table_of(b, seg, data, nkeys != 0)
    rows, stride, base = tb[t], tb[t + 1], u16(tb, t + 2)
    masks = [u16(tb, t + 4 + 2 * r) for r in range(rows)]
    if stride == 1:
        shifts = [struct.unpack("b", bytes([flags & 0xFF]))[0] >> 4]
    else:
        shifts = list(struct.unpack_from(f"{stride}b", tb, t + 4 + 2 * rows))
    comps = []  # (out slot, element index within frame)
    i = 0
    for r, m in enumerate(masks):
        for k in range(16):
            if m & (1 << k):
                comps.append((slot + 16 * r + k, i))
                i += 1
    curves = {s: {} for s, _ in comps}
    if acc is None:
        acc = {}
    for s_, _ in comps:
        acc.setdefault(s_, 0.0)

    def step(off, f, absolute):
        for s, ci in comps:
            if absolute:
                acc[s] = struct.unpack_from("<" + afmt, b, off + asz * (f * stride + ci))[0] * asc
            else:
                sh = shifts[ci] if ci < len(shifts) else shifts[0]
                acc[s] += struct.unpack_from("<" + dfmt, b, off + dsz * (f * stride + ci))[0] * dsc * 2.0 ** -sh

    if nkeys != 0:
        # flat key list: flags & 1 -> deltas, else absolute keys
        # a delta key f gives frame start + 1 + f: the deltas continue from the key already stored at
        # `start` (checked on Kratos's clips: the sum of deltas returns exactly to the frame-0 key)
        lag = 1 if flags & 1 else 0
        for f in range(nkeys):
            step(data + base, f, not flags & 1)
            for s, _ in comps:
                curves[s][start + f + lag] = acc[s]
        return slot, flags, curves, None
    ndelta, nruns = b[data], b[data + 1]
    runs = [struct.unpack_from("<4H", b, data + 2 + 8 * r) for r in range(nruns)]
    events = []
    for r, (cnt, st, ex2, lo2) in enumerate(runs):
        off = seg + base + (((ex2 & 0xC000) << 2) | lo2) + (flags >> 8) * 0x10000
        events.append((st, r >= ndelta, cnt, off))
    # absolute runs seed the accumulators, delta runs integrate in frame order
    for st, absolute, cnt, off in sorted(events, key=lambda e: (e[0], not e[1])):
        for f in range(cnt):
            step(off, f, absolute)
            for s, _ in comps:
                curves[s][st + f + (0 if absolute else 1)] = acc[s]
    return slot, flags, curves, None


def segment_order(b, seg):
    """Sort key for decoding the segments of one clip block in time order with a shared accumulator:
    (first frame, 0 if that frame is an absolute key else 1). Run lists use their earliest run."""
    slot, flags, nkeys, start, ex, lo = struct.unpack_from("<6H", b, seg)
    if nkeys:
        return (start, flags & 1)
    data = seg + (((ex & 0xC000) << 2) | lo) + (flags >> 8) * 0x10000
    ndelta, nruns = b[data], b[data + 1]
    runs = [(struct.unpack_from("<4H", b, data + 2 + 8 * r)[1], 1 if r < ndelta else 0) for r in range(nruns)]
    return min(runs) if runs else (0, 0)


def clips(b):
    ng = u16(b, 0x12)
    if ng == 0 and len(b) > 0x40 and u16(b, 0) == 3:
        # group-only record (level WADs, e.g. ANM_hero_WallClimb): one group at +0x0c, no container
        # header; its clips extend the character's ANM and use its track layout (docs/animation.md)
        go = 0xC
        for c in range(struct.unpack_from("<I", b, go + 0xC)[0]):
            yield go + struct.unpack_from("<I", b, go + 0x34 + 4 * c)[0]
        return
    for g in range(ng):
        go = struct.unpack_from("<I", b, 0x18 + 4 * g)[0]
        for c in range(struct.unpack_from("<I", b, go + 0xC)[0]):
            yield go + struct.unpack_from("<I", b, go + 0x34 + 4 * c)[0]


def main(wad, name):
    data = open(wad, "rb").read()
    b = next(bb for _o, _t, _p, n, bb in wad_records(data) if n == name and bb)
    ntr = u16(b, 0x10)
    tracks = [struct.unpack_from("<HBB", b, 0x18 + 4 * u16(b, 0x12) + 8 * i) for i in range(ntr)]
    sub = sum(t[2] for t in tracks)
    # codec per clip block: a transform track (type 0) owns rot/trans/scale blocks; an emitter track
    # (type 10) owns one block of f32 keys on the emitter parameter block (slot * 4 = byte offset,
    # slot 8 = rate; docs/particles.md section 4). Material tracks use f32 keys too: type 3 RGB
    # multiplier, type 8 UV offset (docs/animation.md "Material tracks"). Other types keep the old guess (block index mod 3).
    kinds = []
    for ttype, _b2, nsub in tracks:
        for j in range(nsub):
            kinds.append(("rot", "trans", "scale")[j % 3] if ttype == 0 else
                         "trans" if ttype in (3, 8, 10) else ("rot", "trans", "scale")[len(kinds) % 3])
    for c in clips(b):
        dur = struct.unpack_from("<f", b, c + 0x14)[0]
        print(f"clip @{c:#x} duration {dur:.2f}s blocks {sub}")
        for k in range(sub):
            blk = c + 0x60 + 16 * k
            _a, nseg, _e, _f, tab, dt = struct.unpack_from("<4HIf", b, blk)
            for s in range(nseg):
                slot, flags, curves, err = decode_segment(b, c + tab + 12 * s, kinds[k])
                for sl, cv in curves.items():
                    fr = sorted(cv)
                    if not fr:
                        print(f"  block {k} seg {s} slot {sl} flags {flags:#x}: {err}")
                        continue
                    vals = [cv[f] for f in fr]
                    samp = " ".join(f"{cv[f]:.3f}" for f in fr[:: max(1, len(fr) // 12)])
                    print(f"  block {k} seg {s} slot {sl} flags {flags:#x} frames {fr[0]}-{fr[-1]} "
                          f"min {min(vals):.3f} max {max(vals):.3f} | {samp}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
