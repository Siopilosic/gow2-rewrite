"""Decode PTC_ particle shapes and FXC_ emitters (docs/particles.md). Research tool.

PTC_ record (loader FUN_00168f50): header, lifetime at +0x70, flags at +0x80, and a blob at +0x98 that
is uploaded to VU1 address 0x16 for program A:
  blob +0x00 qword 0x16: x render routine index (table *0x2d7c18), y entry, z data size, w stride
  blob +0x10 qwords 0x17-0x1e: eight lists of eight u16 (slot k at short (k&3)*2 + (k>>2)); 0/-1 ends
  blob +0x90 qword 0x1f..: data table of vec4
Lists: 0/1 copy state (xyzw/.w), 2/3 random base+range*rand (xyzw/.w), 4/5 route outputs to the GIF
template at 0x222 (xyzw/.w), 6 operators (table *0x2d7c14), 7 operands.

  python tools/ptc_decode.py extracted/pak/RHOD10.WAD PTC_DPNGpart.0 [...]
  python tools/ptc_decode.py extracted/pak/RHOD10.WAD --all
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402

OPS = {0: "const", 1: "const+trail", 2: "linear", 3: "linear-clamp255", 4: "ballistic",
       5: "ballistic+trail", 6: "indirect", 7: "keyframes", 8: "exp", 9: "collide", 10: "collide",
       11: "linear+trail", 12: "ballistic+trail", 13: "collide", 14: "collide", 15: "op15",
       16: "mul", 17: "linear*q0", 18: "ballistic*q0", 19: "op19"}
# operands consumed per op (data entries starting at the list-7 index)
NARGS = {0: 1, 1: 2, 2: 2, 3: 2, 4: 3, 5: 3, 6: 1, 7: 1, 11: 2, 12: 3, 16: 2, 17: 3, 18: 4}
TEMPLATE = {0: "rgba", 1: "pos/size", 2: "rot"}
# render routine index -> what it draws (docs/particles.md section 3.3)
RENDER = {0: "point", 1: "sprite", 2: "billboard (world up)", 3: "billboard (rotated)", 4: "disc fan",
          7: "line streak", 8: "axis billboard", 9: "world quad"}
PRIMS = ["point", "line", "linestrip", "tri", "tristrip", "trifan", "sprite", "?"]
GSREG = {0: "PRIM", 1: "RGBAQ", 2: "ST", 3: "UV", 4: "XYZF2", 5: "XYZ2", 0xE: "A+D", 0xF: "NOP"}


def blend(flags):
    """Shape flags -> GS blend name (docs/particles.md section 3.4: draw pass, then per-shape override)."""
    if flags & 0x6040 and flags & 3 != 3:
        return ("normal", "additive", "subtractive")[flags & 3]
    if flags & 0x4040:
        return "normal"
    if flags & 0x2000:
        return "inherited"
    return ("normal", "additive", "subtractive", "normal")[flags & 3]


def gif_tag(q):
    """16-byte GIF tag -> dict (nloop, prim description, register list)."""
    lo, regs = struct.unpack_from("<QQ", q)
    prim, nreg = lo >> 47 & 0x7FF, lo >> 60 or 16
    bits = [n for b, n in ((3, "gouraud"), (4, "tex"), (5, "fog"), (6, "blend"), (7, "aa"), (8, "uv"))
            if prim >> b & 1]
    return {"nloop": lo & 0x7FFF, "pre": lo >> 46 & 1, "prim": prim, "type": PRIMS[prim & 7],
            "bits": bits, "regs": [GSREG.get(regs >> 4 * i & 0xF, "?") for i in range(nreg)]}


def lists(b):
    out = []
    for g in range(8):
        sh = struct.unpack_from("<8h", b, 0xA8 + 16 * g)
        lst = []
        for k in range(8):
            v = sh[(k & 3) << 1 | k >> 2]
            if v == -1:
                break
            lst.append(v)
        out.append(lst)
    return out


def shape(b):
    """PTC_ record body -> dict (raw fields plus the evaluated program description)."""
    dsize = struct.unpack_from("<I", b, 0xA0)[0]
    data = [struct.unpack_from("<4f", b, 0x128 + 16 * q) for q in range(dsize // 16)]
    L = lists(b)
    prog = []
    for i, (op, arg) in enumerate(zip(L[6], L[7])):
        args = [data[arg + j] if arg + j < len(data) else None for j in range(NARGS.get(op, 1))]
        prog.append({"op": op, "kind": OPS.get(op, "?"), "arg": arg, "operands": args})
    rnd = []
    pairs = iter(range(1, len(data), 2))
    for kind, lst in (("xyzw", L[2]), ("w", L[3])):
        for dst in lst:
            p = next(pairs, None)
            if p is None or p + 1 >= len(data):
                break
            rnd.append({"dst": dst, "lanes": kind, "base": data[p], "range": data[p + 1]})
    return {
        "server": struct.unpack_from("<HH", b, 0),
        "index": struct.unpack_from("<HH", b, 8),
        "matrix": struct.unpack_from("<16f", b, 0x10),
        "shape_name": b[0x54:0x6c].split(b"\0", 1)[0].decode("latin-1"),
        "lifetime": struct.unpack_from("<f", b, 0x70)[0],
        "flags": struct.unpack_from("<I", b, 0x80)[0],
        "n_tail": struct.unpack_from("<I", b, 0x84)[0],
        "layout": b[0x8C:0x98].hex(" "),
        "render": struct.unpack_from("<I", b, 0x9C)[0],
        "lists": L,
        "data": data,
        "random": rnd,
        "program": prog,
        "tail": b[0x128 + dsize:].hex(" ", 4),
        "gif": gif_tag(b[0x128 + dsize:0x138 + dsize]) if len(b) >= 0x138 + dsize else None,
    }


def emitter(b):
    """FXC_ record body -> dict (docs/particles.md section 4)."""
    sub = struct.unpack_from("<HH", b, 0)[1]
    out = {"subtype": sub, "index": struct.unpack_from("<HH", b, 8),
           "matrix": struct.unpack_from("<16f", b, 0x10),
           "shape_name": b[0x54:0x6c].split(b"\0", 1)[0].decode("latin-1")}
    if len(b) >= 0xE4:
        f = struct.unpack_from("<24f", b, 0x84)
        # P = record +0x84 (emitter instance +0xa0, FUN_00132f08). CONFIRMED by code except "axis" (MEDIUM).
        # The survey reading "+0x94 rate, +0x98 speed" was wrong: rate is +0xa4.
        out.update({"axis": f[0:3], "spread": f[3], "speed": f[4], "speed_range": f[5],
                    "radius": f[6:8], "rate": f[8], "params": f})
    return out


def fmt(v):
    return "(" + ", ".join(f"{x:.4g}" for x in v) + ")" if v else "-"


def main(argv):
    data = open(argv[0], "rb").read()
    want = set(argv[1:])
    recs = list(wad_records(data))
    # a shape's material is the MAT_ reference inside its group: GroupStart, PTC_x, MAT_y, GroupEnd
    mats = {n: recs[i + 1][3] for i, (_o, _t, _p, n, b) in enumerate(recs[:-1])
            if b and n.startswith("PTC_") and recs[i + 1][3].startswith("MAT_")}
    for _o, _t, _p, n, b in recs:
        if not b or not (("--all" in want and n.startswith(("PTC_", "FXC_"))) or n in want):
            continue
        if n.startswith("PTC_"):
            s = shape(b)
            print(f"{n}: shape {s['shape_name']!r} life {s['lifetime']:.3g}s flags {s['flags']:#x} "
                  f"render {s['render']} ({RENDER.get(s['render'], '?')}) material {mats.get(n, '-')}")
            if s["gif"]:
                g = s["gif"]
                print(f"   gif {g['type']} {'+'.join(g['bits'])} regs {','.join(g['regs'])}  blend {blend(s['flags'])}"
                      f"{' zwrite' if s['flags'] & 0x20000 else ''}{' screen-texture' if 'tex' in g['bits'] and not s['flags'] & 0x80 else ''}")
            for r in s["random"]:
                print(f"   random q{r['dst']}.{r['lanes']} = {fmt(r['base'])} + {fmt(r['range'])} * rand")
            for i, p in enumerate(s["program"]):
                print(f"   out{i}: {p['kind']} q{p['arg']} {' '.join(fmt(a) for a in p['operands'])}")
            print(f"   route xyzw {s['lists'][4]}  .w {s['lists'][5]}  state {s['lists'][0]} / .w {s['lists'][1]}")
        else:
            e = emitter(b)
            extra = "" if "params" not in e else (
                f" axis {fmt(e['axis'])} rate {e['rate']:.3g}/s speed {e['speed'] * 16:.3g}"
                f"+-{e['speed_range'] * 8:.3g} spread {e['spread'] * 90:.3g}deg radius {fmt(e['radius'])}")
            print(f"{n}: FX subtype {e['subtype']} -> {e['shape_name']!r} at {fmt(e['matrix'][12:15])}{extra}")


if __name__ == "__main__":
    main(sys.argv[1:])
