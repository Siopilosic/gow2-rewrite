"""Oracle summaries for the Rust skin module (rigs, joint-bound meshes, clip channels).

  python tools/oracle_skin.py gow2-rs/crates/gow2-formats/tests/oracle/skin.tsv

Per rig of each WAD: joint count, hash of parents / matrices / names / TRS, joint-bound mesh counts and hash,
and the channel hash of the first clip and of the first named clip. Only derived numbers are stored.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from rig_anim import (clip_channels, clip_name, find_rigs, mat_to_trs, mesh_joints, parse_rig,  # noqa: E402
                      rig_joint_names)
from anm_decode import clips  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WADS = ["R_HERO00", "R_HERO01", "R_ZEUS", "RHOD10", "ATLAS210"]
M = 0xFFFFFFFFFFFFFFFF


class H:
    def __init__(self):
        self.h = 1469598103934665603

    def i(self, x):
        self.h = ((self.h ^ (int(x) & M)) * 1099511628211) & M

    def f32(self, x):
        self.i(struct.unpack("<I", struct.pack("<f", x))[0])

    def f64(self, x):
        self.i(struct.unpack("<Q", struct.pack("<d", x))[0])

    def s(self, t):
        for c in t.encode("latin-1"):
            self.i(c)
        self.i(0xFF)


def hash_clip(res):
    h = H()
    if res is None:
        return "none"
    dt, dur, ch = res
    h.f32(dt)
    h.f32(dur)
    for j in sorted(ch):
        h.i(j)
        for ki, kind in enumerate(("rot", "trans", "scale")):
            comps = ch[j].get(kind, {})
            h.i(ki)
            for c in sorted(comps):
                h.i(c)
                for f in sorted(comps[c]):
                    h.i(f)
                    h.f64(comps[c][f])
    return str(h.h)


with open(sys.argv[1], "w", encoding="utf-8", newline="") as out:
    out.write("wad\tmodel\tjoints\trig\tverts\ttris\tmesh\tclip0\tnamed\n")
    for w in WADS:
        p = os.path.join(ROOT, "extracted", "pak", w + ".WAD")
        if not os.path.exists(p):
            continue
        seq, recs = [], {}
        for _o, tag, _p, n, body in wad_records(open(p, "rb").read()):
            seq.append((tag, n, body))
            if tag == 1 and body:
                recs.setdefault(n, body)
        for model, (rigb, anm_name) in find_rigs(seq).items():
            try:
                parents, mats = parse_rig(rigb)
                names = rig_joint_names(rigb)
                nj = len(parents)
                h = H()
                for x in parents:
                    h.i(x)
                for m in mats:
                    for x in m:
                        h.f32(x)
                    t, q, s = mat_to_trs(m)
                    for x in (*t, *q, *s):
                        h.f64(x)
                for nm in names:
                    h.s(nm)
                blob = recs.get(f"MDL_{model}_0")
                vs, tris, mh = 0, 0, "none"
                if blob and len(blob) > 64:
                    V, UV, COL, J, T = mesh_joints(blob)
                    mhh = H()
                    for v in V:
                        for x in v:
                            mhh.i(x)
                    for uv in UV:
                        for x in uv:
                            mhh.f64(x)
                    for c in COL:
                        for x in c:
                            mhh.i(x)
                    for x in J:
                        mhh.i(x)
                    for t_ in T:
                        for x in t_:
                            mhh.i(x)
                    vs, tris, mh = len(V), len(T), str(mhh.h)
                c0 = named = "none"
                anm = recs.get(anm_name) if anm_name else None
                if anm and len(anm) > 0x40:
                    c0 = hash_clip(clip_channels(anm, nj))
                    found = []
                    for c in clips(anm):
                        nm = clip_name(anm, c)
                        if not nm or any(nm == f[0] for f in found):
                            continue
                        res = clip_channels(anm, nj, nm)
                        if res and res[2]:
                            found.append((nm, hash_clip(res)))
                        if len(found) == 3:
                            break
                    named = ",".join(f"{a}:{b}" for a, b in found) or "none"
                out.write(f"{w}\t{model}\t{nj}\t{h.h}\t{vs}\t{tris}\t{mh}\t{c0}\t{named}\n")
            except (struct.error, IndexError):
                continue