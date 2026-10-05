"""Rigs, joint-bound meshes and ANM_ clip channels for the glTF export (docs/animation.md).

Rig record (unnamed tag-1 record that follows a go<name> node; header u16 1, u16 1):
  +0x04 u32 joint count nj; joint entries 16 B at +0x18: s16 firstChild, nextSibling, parent, ?
  names follow; X = align16(0x18 + 40*nj); local matrices (row-vector, 64 B, translation in row 3)
  at X + 0x30.  HIGH (3 rigs: 1, 2 and 7 joints; parent links and lamp geometry agree).
Joint binding: each model part C has a palette of u16(C+10) s32 joint indices after its DMA entry
  table (C + 0x20 + entries*16; EE draw loop FUN_00173b28). Every GIF tag in a batch header covers
  NLOOP vertices and its w word selects the palette slot: slot = (w & 0x3ff) / 4 (VU program B loads
  the matrix at base + 16 + w). Vertices are joint-local. HIGH (lamp: 485/484 verts on the two lamps).
"""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from anm_decode import clips, decode_segment, segment_order, u16  # noqa: E402
from mdl_decode import batches, parts  # noqa: E402


def find_rigs(seq):
    """model name -> (rig bytes, ANM name or None). A go<name> group holds {rig, ANM_ ref, MDL_ ref, ESC_ ref}."""
    out = {}
    for i, (tag, n, body) in enumerate(seq):
        if tag != 1 or n or not body or len(body) < 0x28 or struct.unpack_from("<HH", body) != (1, 1):
            continue
        anm = mdl = None
        for tag2, n2, b2 in seq[i + 1:i + 12]:
            if tag2 != 1 or b2:
                break
            if n2.startswith("ANM_") and anm is None:
                anm = n2
            elif n2.startswith("MDL_") and mdl is None:
                mdl = n2[4:]
        if mdl and mdl not in out:
            out[mdl] = (body, anm)
    return out


def rig_joint_names(b):
    """Joint names: 24-byte strings from 0x14 + 16*nj (they start 4 bytes before the end of the joint
    entries; checked on Kratos's 123-joint rig, docs/animation.md)."""
    nj = struct.unpack_from("<I", b, 4)[0]
    base = 0x14 + 16 * nj
    return [b[base + 24 * i:base + 24 * (i + 1)].split(bytes(1))[0].decode("latin-1") for i in range(nj)]


def parse_rig(b):
    nj = struct.unpack_from("<I", b, 4)[0]
    parents = [struct.unpack_from("<h", b, 0x18 + 16 * j + 4)[0] for j in range(nj)]
    x = (0x18 + 40 * nj + 15) & ~15
    mats = [struct.unpack_from("<16f", b, x + 0x30 + 64 * j) for j in range(nj)]
    return parents, mats


def mat_to_trs(m):
    """Row-vector GoW matrix -> glTF TRS (column-vector rotation = transpose)."""
    t = list(m[12:15])
    rows = [m[0:3], m[4:7], m[8:11]]
    # explicit left-to-right sums: Python 3.12+ sum() is compensated, which differs in the last bit from a plain f64 sum
    s = [math.sqrt(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]) or 1.0 for r in rows]
    # column-vector rotation matrix R[i][j] = row-vector M[j][i] / scale
    R = [[rows[j][i] / s[j] for j in range(3)] for i in range(3)]
    tr = R[0][0] + R[1][1] + R[2][2]
    if tr > 0:
        w = math.sqrt(1 + tr) * 2
        q = [(R[2][1] - R[1][2]) / w, (R[0][2] - R[2][0]) / w, (R[1][0] - R[0][1]) / w, w / 4]
    elif R[0][0] > R[1][1] and R[0][0] > R[2][2]:
        w = math.sqrt(1 + R[0][0] - R[1][1] - R[2][2]) * 2
        q = [w / 4, (R[0][1] + R[1][0]) / w, (R[0][2] + R[2][0]) / w, (R[2][1] - R[1][2]) / w]
    elif R[1][1] > R[2][2]:
        w = math.sqrt(1 + R[1][1] - R[0][0] - R[2][2]) * 2
        q = [(R[0][1] + R[1][0]) / w, w / 4, (R[1][2] + R[2][1]) / w, (R[0][2] - R[2][0]) / w]
    else:
        w = math.sqrt(1 + R[2][2] - R[0][0] - R[1][1]) * 2
        q = [(R[0][2] + R[2][0]) / w, (R[1][2] + R[2][1]) / w, w / 4, (R[1][0] - R[0][1]) / w]
    n = math.sqrt(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]) or 1.0
    return t, [c / n for c in q], s


def mesh_joints(blob, uvscale=4096.0):
    """Like textured_preview.mesh_full, plus a joint index per vertex (group 0 only)."""
    V, UV, COL, J, T = [], [], [], [], []
    for (i, j, k, C, kind, pk) in parts(blob):
        slot = struct.unpack_from("<I", blob, C + 8)[0] & 0xFFFF
        n10 = u16(blob, C + 10)
        nent = blob[C + 0x18] * struct.unpack_from("<I", blob, C + 0xC)[0] * struct.unpack_from("<I", blob, C + 4)[0]
        pal = struct.unpack_from(f"<{n10}i", blob, C + 0x20 + nent * 16) if n10 else (0,)
        for g, s, e in pk:
            if g != 0:
                continue
            for b in batches(blob, s, e):
                if "pos" not in b:
                    continue
                base = len(V)
                p = b["pos"]
                V += [(v[0], v[1], v[2]) for v in p]
                UV += [(u / uvscale, v / uvscale) for u, v in b.get("uv", [(0, 0)] * len(p))]
                COL += b.get("col", [(128, 128, 128, 128)] * len(p))
                jv, vi = [], 0
                for tag in b.get("hdr", []):
                    nl = tag[0] & 0x7FFF
                    ps = (tag[3] & 0x3FF) // 4
                    jv += [pal[ps] if ps < len(pal) else pal[0]] * nl
                    vi += nl
                J += (jv + [pal[0]] * len(p))[:len(p)]
                for n in range(2, len(p)):
                    if not (p[n][3] & 0x8000):
                        t = (base + n - 2, base + n - 1, base + n)
                        t = t if n % 2 == 0 else (t[1], t[0], t[2])
                        T.append((*t, slot))
    return V, UV, COL, J, T


def clip_name(anm, c):
    """Clip name: string at clip + 0x24 (hash at + 0x20 = gow_hash(name) for 193 of Kratos's 355 named
    clips; the rest are not plain clips). Returns "" when the bytes are not a name."""
    raw = anm[c + 0x24:c + 0x3c].split(bytes(1))[0]
    return raw.decode("latin-1") if raw and all(48 <= x < 123 or x in (46, 95) for x in raw) else ""


def clip_channels(anm, nj, name=None):
    """A clip of a transform-track ANM -> (dt, duration, {joint: {"rot"|"trans"|"scale": {comp: {frame: v}}}}).
    `name` selects a clip by its name (clip_name); default is the first clip."""
    ng, nt = u16(anm, 0x12), u16(anm, 0x10)
    if ng == 0:
        tracks = [(0, 0, 3)]  # group-only record: the character ANM's transform track (anm_decode.clips)
    else:
        tracks = [struct.unpack_from("<HBB", anm, 0x18 + 4 * ng + 8 * i) for i in range(nt)]
    if not tracks or tracks[0][0] != 0:
        return None
    if name is None:
        c = next(clips(anm), None)
    else:
        c = next((x for x in clips(anm) if clip_name(anm, x) == name), None)
    if c is None:
        return None
    dur = struct.unpack_from("<f", anm, c + 0x14)[0]
    out, dt = {}, 1 / 30
    for k, kind in enumerate(("rot", "trans", "scale")[:tracks[0][2]]):
        _a, nseg, _e, _f, tab, dt = struct.unpack_from("<4HIf", anm, c + 0x60 + 16 * k)
        shared = {}  # one accumulator per slot for the whole block (see decode_segment)
        segs = [c + tab + 12 * s for s in range(nseg)]
        # time order (anm_decode.segment_order): by first frame, absolute keys before deltas
        # (Kratos's clips split one channel into a key segment, a delta segment and an end-key segment)
        segs.sort(key=lambda sg: segment_order(anm, sg))
        for sg in segs:
            try:
                _slot, _flags, curves, _err = decode_segment(anm, sg, kind, shared)
            except struct.error:
                continue
            for sl, cv in curves.items():
                jnt, comp = divmod(sl, 4)
                if jnt < nj and cv:
                    out.setdefault(jnt, {}).setdefault(kind, {}).setdefault(comp, {}).update(cv)
    return dt, dur, out
