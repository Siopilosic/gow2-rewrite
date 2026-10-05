"""Survey VU1 shader-variant selection over all model parts on disc (docs/rendering.md §1.1).

The EE draw loop (FUN_00173b28) computes the variant index as
    v = ((MAT +0x40) [| 0x20 if the instance flag byte is set]) & (part +0x14) | (part +0x10)
and loads fragment DAT_002e2200[v & 0x7f] from program C. This tool reports part +0x10/+0x14
values and the resulting v (without the per-instance 0x20 bit) per level.

  python tools/variant_survey.py extracted/pak analysis/vu/variant_survey.tsv
"""
import collections
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from mdl_decode import parts  # noqa: E402
from textured_preview import model_materials  # noqa: E402


def main(src, out):
    rows = []
    tot = collections.Counter()
    for fn in sorted(os.listdir(src)):
        if not fn.upper().endswith(".WAD"):
            continue
        data = open(os.path.join(src, fn), "rb").read()
        seq = [(tag, n, b) for _o, tag, _p, n, b in wad_records(data)]
        recs = {}
        for tag, n, b in seq:
            if tag == 1 and b:
                recs.setdefault(n, b)
        for n, blob in recs.items():
            if not (n.startswith("MDL_") and n.endswith("_0")):
                continue
            name = n[4:-2]
            mats = model_materials(seq, name)
            try:
                plist = list(parts(blob))
            except Exception:
                continue
            for (i, j, k, C, kind, pk) in plist:
                if kind not in (0x18, 0x0E):
                    continue
                orv, andv = struct.unpack_from("<II", blob, C + 0x10)
                slot = struct.unpack_from("<I", blob, C + 8)[0] & 0xFFFF
                mn = mats[slot] if slot < len(mats) else ""
                mb = recs.get(mn)
                m40 = struct.unpack_from("<I", mb, 0x40)[0] if mb and len(mb) == 120 else None
                v = None if m40 is None else ((m40 & andv) | orv) & 0x7F
                rows.append((fn[:-4], name, k, kind, f"{orv:08x}", f"{andv:08x}", mn,
                             "" if m40 is None else f"{m40:08x}", "" if v is None else f"{v:02x}"))
                tot[(fn[:3], v)] += 1
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        f.write("wad\tmodel\tpart\tkind\tor10\tand14\tmat\tmat40\tvariant\n")
        for r in rows:
            f.write("\t".join(map(str, r)) + "\n")
    print(len(rows), "parts")
    c = collections.Counter(r[8] for r in rows)
    print("variants:", c.most_common())
    print("or/and:", collections.Counter((r[4], r[5]) for r in rows).most_common(12))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
