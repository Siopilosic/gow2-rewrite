"""God of War II static world collision: the `RIB_sheet` record of a level WAD (collision server 0x10, header type 0x10).

Layout from the loader FUN_00179338 (docs/collision.md section 3):
  +0x10 f32[3] bounds min, +0x20 f32[3] bounds max
  +0x3e u16 triangles, +0x40 u16 quads, +0x42 u16 vertices, +0x48 u16 surfaces, +0x4a u16 flag names
  +0x50 u32[8] section offsets: 0 broad phase (not decoded), 1 surfaces (64 bytes each), 2 flag-name table (76 bytes each),
        3 leaf lists (not decoded), 4 triangles (8 bytes: u16 surface<<4, v0 v1 v2), 5 quads (10 bytes: u16 surface<<4, v0..v3),
        6 vertices (f32 x y z), 7 end
  surface record: +0x00 name[24], +0x18 u32 flags (bit names in the flag-name table), +0x1c u32 flags high

  python tools/sheet_decode.py <file.WAD>      # prints surfaces, flag names and polygon statistics
"""
import math
import struct
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402


def cstr(b):
    return b.split(b"\0")[0].decode("latin1")


def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _unit(n):
    l = math.sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2])
    return (n[0] / l, n[1] / l, n[2] / l) if l > 1e-9 else (0.0, 0.0, 0.0)


def parse(p):
    """Returns dict(bounds, surfaces=[(name, flags, flags_hi)], flag_names, polys=[(surface, corners, normal)])."""
    if len(p) < 0x70 or struct.unpack_from("<I", p, 0)[0] != 0x10:
        return None
    off = struct.unpack_from("<8I", p, 0x50)
    ntri, nquad, nvert = struct.unpack_from("<3H", p, 0x3e)
    nsurf, nflag = struct.unpack_from("<2H", p, 0x48)
    stride = (off[2] - off[1]) // nsurf if nsurf else struct.unpack_from("<H", p, 0x44)[0]
    verts = [struct.unpack_from("<3f", p, off[6] + 12 * i) for i in range(nvert)]
    surfaces = []
    for i in range(nsurf):
        o = off[1] + stride * i
        surfaces.append((cstr(p[o:o + 24]), *struct.unpack_from("<2I", p, o + 0x18)))
    flag_names = [cstr(p[off[2] + 76 * i:off[2] + 76 * i + 0x3c]) for i in range(nflag)]
    polys = []
    for i in range(ntri):
        s, a, b, c = struct.unpack_from("<4H", p, off[4] + 8 * i)
        va, vb, vc = verts[a], verts[b], verts[c]
        polys.append((s >> 4, [va, vb, vc], _unit(_cross(_sub(vb, va), _sub(vc, va)))))
    for i in range(nquad):
        s, a, b, c, d = struct.unpack_from("<5H", p, off[5] + 10 * i)
        va, vb, vc, vd = verts[a], verts[b], verts[c], verts[d]
        polys.append((s >> 4, [va, vb, vc, vd], _unit(_cross(_sub(vc, va), _sub(vd, vb)))))
    bmin = struct.unpack_from("<3f", p, 0x10)
    bmax = struct.unpack_from("<3f", p, 0x20)
    return {"bounds": (bmin, bmax), "surfaces": surfaces, "flag_names": flag_names, "polys": polys, "verts": verts}


def find(data):
    for _o, tag, _p, name, body in wad_records(data):
        if tag == 1 and name == "RIB_sheet":
            return parse(body)
    return None


if __name__ == "__main__":
    s = find(open(sys.argv[1], "rb").read())
    if not s:
        print("no RIB_sheet")
        sys.exit(1)
    print("bounds", s["bounds"])
    print(len(s["polys"]), "polygons,", len(s["verts"]), "vertices")
    for i, (n, f, h) in enumerate(s["surfaces"]):
        cnt = sum(1 for q in s["polys"] if q[0] == i)
        print(f"  {i:2d} {n:32s} flags={f:08x} hi={h:08x} polys={cnt}")
    print("flag names:", ", ".join(s["flag_names"]))
