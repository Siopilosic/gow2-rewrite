"""Export a WAD's particle shapes and emitters for the level viewer (docs/particles.md). Research export.

  python tools/ptc_export.py extracted/pak/RHOD10.WAD analysis/levels/RHOD10_gltf

Writes <out>/particles.json and <out>/tex/ptc_<MAT>.png. The viewer evaluates each shape's op program on
the CPU (the same lists and data table that program A gets), so the JSON carries the raw lists.

Decoded facts used: PTC_ layout and program lists (sections 3, 3.1), render routines and material
binding (3.3), flags/blend (3.4), FXC_ parameter block P = record +0x84 (4), emitter rate tracks =
ANM_ track type 10 (4). Assumptions, also written to the JSON "assumptions" list:
  - (resolved 2026-10-03) rig emitters hang under joint record +0x08 and their rate track is ANM block
    record +0x0a (docs/particles.md 4.3);
  - the subtype-1 spawn cone is around the direction P+0x00 (FUN_00133780 frame, FUN_00133970 cone).
Surface/curve emitters (subtypes 3/4) carry "geom", the name of a subtype-13 geometry in "geoms" (4.2).
"""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from anm_decode import clips, decode_segment, u16  # noqa: E402
from dcparse import wad_records  # noqa: E402
from gfx_decode import TextureStore  # noqa: E402
from gltf_export import write_png_rgba  # noqa: E402
from ptc_decode import blend, emitter, lists, shape  # noqa: E402


def rig_groups(seq):
    """Unnamed tag-1 rig records (header u16 1, u16 1) -> (MDL name, ANM name, [FXC names in order])."""
    out = []
    go = None
    for i, (tag, n, body) in enumerate(seq):
        if tag == 1 and n.lower().startswith("go") and body and struct.unpack_from("<HH", body) == (1, 3):
            go = n  # the go node (subtype 3) whose group holds this rig
        if tag != 1 or n or not body or len(body) < 0x28 or struct.unpack_from("<HH", body) != (1, 1):
            continue
        mdl = anm = None
        fxc = []
        for tag2, n2, b2 in seq[i + 1:]:
            if tag2 != 1 or b2:
                break
            if n2.startswith("MDL_") and mdl is None:
                mdl = n2[4:]
            elif n2.startswith("ANM_") and anm is None:
                anm = n2
            elif n2.startswith("FXC_"):
                fxc.append(n2)
        if fxc:
            out.append((mdl, anm, fxc, go))
    return out


def rate_tracks(anm):
    """Type-10 tracks of clip 0 -> {ANM block index: {"dt", "duration", "keys": [[frame, value], ...]}} (slot 8 = rate)."""
    ng, nt = u16(anm, 0x12), u16(anm, 0x10)
    tracks = [struct.unpack_from("<HBB", anm, 0x18 + 4 * ng + 8 * i) for i in range(nt)]
    c = next(clips(anm), None)
    if c is None:
        return []
    dur = struct.unpack_from("<f", anm, c + 0x14)[0]
    out, blk = {}, 0
    for ttype, _b2, nsub in tracks:
        for _j in range(nsub):
            if ttype == 10:
                _a, nseg, _e, _f, tab, dt = struct.unpack_from("<4HIf", anm, c + 0x60 + 16 * blk)
                keys = {}
                for s in range(nseg):
                    try:
                        _sl, _fl, curves, _err = decode_segment(anm, c + tab + 12 * s, "trans")
                    except (struct.error, IndexError):  # BOG40 has a truncated segment
                        continue
                    keys.update(curves.get(8, {}))
                out[blk] = {"dt": dt, "duration": dur, "keys": [[f, round(v, 4)] for f, v in sorted(keys.items())]}
            blk += 1
    return out


def main(wad, out):
    os.makedirs(os.path.join(out, "tex"), exist_ok=True)
    data = open(wad, "rb").read()
    recs_l = list(wad_records(data))
    seq = [(t, n, b) for _o, t, _p, n, b in recs_l]
    recs = {}
    for t, n, b in seq:
        if t == 1 and b:
            recs.setdefault(n, b)
    store = TextureStore(recs)
    mats = {seq[i][1]: seq[i + 1][1] for i in range(len(seq) - 1)
            if seq[i][2] and seq[i][1].startswith("PTC_") and seq[i + 1][1].startswith("MAT_")}

    shapes, by_shape_name, tex_done = {}, {}, {}
    for n, b in recs.items():
        if not n.startswith("PTC_"):
            continue
        s = shape(b)
        mat = mats.get(n)
        uri = None
        if mat and mat not in tex_done:
            t = store.material_texture(mat)
            if t:
                w, h, rgba = t
                uri = "tex/ptc_" + "".join(c if c.isalnum() or c in "_-." else "_" for c in mat) + ".png"
                write_png_rgba(os.path.join(out, uri), w, h, rgba)
            tex_done[mat] = uri
        uri = tex_done.get(mat)
        shapes[n] = {"shapeName": s["shape_name"], "life": s["lifetime"], "flags": s["flags"],
                     "render": s["render"], "blend": blend(s["flags"]), "zwrite": bool(s["flags"] & 0x20000),
                     "material": mat, "tex": uri, "lists": lists(b),
                     "data": [[round(x, 5) for x in q] for q in s["data"]],
                     "prim": s["gif"]["type"] if s["gif"] else None}
        by_shape_name.setdefault(s["shape_name"], n)

    rig_of, track_of, go_of = {}, {}, {}
    for mdl, anm, fxc, go in rig_groups(seq):
        tr = rate_tracks(recs[anm]) if anm in recs else {}
        for f in fxc:
            # a go group without an MDL_ (gofireball03) is a runtime effect spawned by scripts: no placement
            rig_of.setdefault(f, mdl or "")
            go_of.setdefault(f, []).append(go)
            # record +0x0a = the ANM channel (block index) that drives this emitter's P block
            # (FUN_0013bd08 -> instance +0x62, matched by FUN_0013be80; docs/particles.md 4.3)
            ch = struct.unpack_from("<H", recs[f], 0x0a)[0] if f in recs else 0xFFFF
            if ch in tr:
                track_of.setdefault(f, tr[ch] | {"anm": anm})

    # emission geometry (FXC_ subtype 13, FUN_00132588; docs/particles.md 4.2): record +0x70 shape name,
    # +0x54 & 3 = 0 curve (NCV_ named at +0x58) or 1 mesh (MSH_ named at +0x58), +0x10 matrix
    raw = {}
    for _o, t, _p, n, b in recs_l:
        if b and n.startswith(("NCV_", "MSH_")):
            raw.setdefault(n, b)
    geoms = {}
    for n, b in recs.items():
        if not n.startswith("FXC_") or len(b) < 0x88 or struct.unpack_from("<HH", b)[1] != 13:
            continue
        gname = b[0x70:0x88].split(b"\0", 1)[0].decode("latin-1")
        ref = b[0x58:0x70].split(b"\0", 1)[0].decode("latin-1")
        kind = struct.unpack_from("<I", b, 0x54)[0] & 3
        g = {"record": n, "ref": ref, "matrix": [round(x, 5) for x in struct.unpack_from("<16f", b, 0x10)]}
        rb = raw.get(ref)
        if rb is None:
            continue
        if kind == 0:
            # NCV_: u32 n, 12 bytes pad, n 4x4 segment matrices, n knots (global parameter, FUN_00135528)
            ns = struct.unpack_from("<I", rb, 0)[0]
            g["kind"] = "curve"
            g["segs"] = [round(x, 5) for x in struct.unpack_from(f"<{16 * ns}f", rb, 0x10)]
            g["knots"] = [round(x, 6) for x in struct.unpack_from(f"<{ns}f", rb, 0x10 + 0x40 * ns)]
        elif kind == 1:
            # MSH_: u32 vertex count, u32 triangle count, two runtime pointers, vertices (position, normal:
            # 6 floats), triangles (3 u16 indices + u16 cumulative area x 65535; FUN_00135e48)
            nv, nt = struct.unpack_from("<II", rb, 0)
            g["kind"] = "mesh"
            g["verts"] = [round(x, 5) for x in struct.unpack_from(f"<{6 * nv}f", rb, 0x10)]
            g["tris"] = list(struct.unpack_from(f"<{4 * nt}H", rb, 0x10 + 0x18 * nv))
        else:
            continue
        geoms.setdefault(gname, g)

    emitters = []
    for n, b in recs.items():
        # every 228-byte emitter record (FUN_00132670 builds them all); 11 = trail, 12 = field, 13 = geometry
        if not n.startswith("FXC_") or struct.unpack_from("<HH", b)[1] in (11, 12, 13) or len(b) < 0xE4:
            continue
        e = emitter(b)
        emitters.append({"name": n, "subtype": e["subtype"], "shape": by_shape_name.get(e["shape_name"]),
                         "matrix": [round(x, 5) for x in e["matrix"]], "spread": e["spread"],
                         "speed": e["speed"], "speedRange": e["speed_range"], "radius": list(e["radius"]),
                         "rate": e["rate"], "P": [round(x, 5) for x in e["params"]], "rig": rig_of.get(n) or None, "rateTrack": track_of.get(n),
                         "placed": rig_of.get(n) != "", "go": go_of.get(n, []),
                         # record +0x08 = parent joint of the owner's skeleton, -1 = none (FUN_0013c010)
                         "joint": struct.unpack_from("<h", b, 8)[0]})
        if e["subtype"] in (3, 4):
            # surface / curve emitters name their geometry's shape at record +0x6c
            emitters[-1]["geom"] = b[0x6c:0x84].split(b"\0", 1)[0].decode("latin-1") or None

    doc = {"source": os.path.basename(wad), "shapes": shapes, "emitters": emitters, "geoms": geoms,
           "assumptions": ["rig emitters hang under joint record +0x08; rate track = ANM block record +0x0a (docs/particles.md 4.3)",
                           "subtype-1 spawn cone around the direction P+0x00",
                           "surface/curve geometry drawn in its record frame under the emitter parent; instance +0xd0/+0xe0 not applied",
                           "particle size is screen-relative: world half-extent = size * 2047.5/256 * tan(fov_x/2) (docs/particles.md 3.3.1)"]}
    json.dump(doc, open(os.path.join(out, "particles.json"), "w"))
    print(f"{len(shapes)} shapes, {len(emitters)} emitters ({sum(1 for e in emitters if e['rig'])} on rigs, "
          f"{sum(1 for e in emitters if e['rateTrack'])} with rate tracks), {sum(1 for v in tex_done.values() if v)} textures")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
