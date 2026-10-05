"""Per-move visual-effect timeline of a character's move data (docs/effects.md).

  python tools/move_fx.py extracted/pak/R_HERO00.WAD

Writes analysis/dc/<WAD>/fx.tsv with one row per tActionPlayFX and tActionFootEffect action:
  move, action, trigger (docs/combat.md §6), window start/end (normalised move time),
  effect (go record name, resolved by hash over every WAD record name), wads (where the effect lives),
  joint (attach joint name, from the character's rig), flags, rotation (degrees X,Y,Z).

PlayFX (kind 0x0d, class SCR_PlayFX): +0x08 class hash, +0x0c relative pointer to the parameter block
(the block SCR_PlayFX reads at instance +0x74, docs/scripting.md §8): +0x00 effect hash, +0x04 joint hash,
+0x24/+0x28/+0x2c rotation in degrees, +0x30 u16 flags.
FootEffect (kind 0x1d, case 0x00248cd4): +0x08 u32 flags (low nibble = gait override 0 auto, 1 step,
2 walk, 3 run, 4 land; 0x100/0x200 = also a ground effect at the joint), +0x0c foot joint hash.
"""
import collections
import glob
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import DC, gow_hash, wad_records  # noqa: E402
from rig_anim import rig_joint_names  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), "..")
GAIT = {0: "auto", 1: "step", 2: "walk", 3: "run", 4: "land"}


def half(h):
    return struct.unpack("<e", struct.pack("<H", h))[0]


def main():
    path = sys.argv[1]
    wad = os.path.splitext(os.path.basename(path))[0]
    names, where = {}, collections.defaultdict(set)
    for w in glob.glob(os.path.join(ROOT, "extracted", "pak", "*.WAD")):
        try:
            for _o, _t, _p, n, _b in wad_records(open(w, "rb").read()):
                if n:
                    names.setdefault(gow_hash(n), n)
                    where[n].add(os.path.splitext(os.path.basename(w))[0])
        except (struct.error, UnicodeDecodeError):
            continue
    # joint names of every rig in the character WAD (rig records: unnamed tag-1, header u16 1, u16 1)
    for _o, t, _p, n, b in wad_records(open(path, "rb").read()):
        if t == 1 and not n and b and len(b) > 0x28 and struct.unpack_from("<HH", b) == (1, 1):
            try:
                for j in rig_joint_names(b):
                    names.setdefault(gow_hash(j), j)
            except (struct.error, UnicodeDecodeError, ValueError):
                pass

    dc = DC(path, None)
    B = dc.blob
    objs = sorted(dc.objects)
    # action -> owning move (actions.tsv order), via the move's action list
    owner = {}
    for o, n, t in objs:
        if t != 0x62:
            continue
        w = struct.unpack_from("<I", B, o + 0x18)[0]
        arr = o + 0x18 + (struct.unpack_from("<i", B, o + 0x18)[0] >> 12)
        for k in range(w & 0xFFF):
            v = struct.unpack_from("<i", B, arr + 4 * k)[0]
            if v:
                owner.setdefault(arr + 4 * k + v, []).append(n)

    def nm(h):
        return "" if h == 0 else names.get(h, f"{h:08x}")

    rows, per = [], collections.Counter()
    for o, n, _t in objs:
        if n.startswith("tActionPlayFX"):
            p = o + 0x0C + struct.unpack_from("<i", B, o + 0x0C)[0]
            fx, joint = struct.unpack_from("<II", B, p)
            rot = ",".join(f"{x:g}" for x in struct.unpack_from("<3f", B, p + 0x24))
            flags = struct.unpack_from("<H", B, p + 0x30)[0]
            kind, eff, jn, fl = "PlayFX", nm(fx), nm(joint), f"{flags:#x}"
        elif n.startswith("tActionFootEffect"):
            flags, joint = struct.unpack_from("<II", B, o + 8)
            rot = ""
            kind, eff, jn = "FootEffect", "(surface)", nm(joint)
            fl = f"{flags:#x} {GAIT.get(flags & 0xF, flags & 0xF)}"
        else:
            continue
        per[eff] += 1
        for mv in owner.get(o, [""]):
            rows.append((mv, kind, str(B[o + 2]), f"{half(struct.unpack_from('<H', B, o + 4)[0]):.4g}",
                         f"{half(struct.unpack_from('<H', B, o + 6)[0]):.4g}", eff,
                         ",".join(sorted(where.get(eff, []))[:4]) + ("…" if len(where.get(eff, [])) > 4 else ""),
                         jn, fl, rot))
    out = os.path.join(ROOT, "analysis", "dc", wad)
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "fx.tsv"), "w", encoding="utf-8", newline="") as fh:
        fh.write("move\tkind\ttrigger\twin_start\twin_end\teffect\twads\tjoint\tflags\trotation\n")
        for r in rows:
            fh.write("\t".join(r) + "\n")
    print(f"{wad}: {len(rows)} effect rows, {len(per)} distinct effects,",
          sum(1 for e in per if all(c in "0123456789abcdef" for c in e) and len(e) == 8), "unresolved")


if __name__ == "__main__":
    main()
