"""Dump the level cameras (CAM_ records) and camera zones (CMZ_ records) of a WAD.

  python tools/cam_dump.py extracted/pak/RHOD10.WAD

Writes analysis/cameras/<WAD>/cameras.tsv, transitions.tsv and zones.tsv. Layouts: docs/camera.md §4.

CAM_ record (server 9, WAD param 26). The camera class constructor FUN_0014c060 copies 100 bytes from
record +0x80 to class +0x10, so class offset = record offset - 0x70.
  +0x00 u32 9, +0x10..+0x4c node matrix (rotation rows, position at +0x40)
  +0x50 char[24] rail curve name (NCV_*, looked up by FUN_0014b9d0 into class +0x94)
  +0x68 char[24] second name (class +0x98 list)
  +0x88 u32 flags (class +0x18), +0x8c/+0x8e u16 axis modes (class +0x1c/+0x1e)
  +0x90 distance factor, +0x94/+0x98 distance max/min, +0x9c/+0xa0 second distance clamp (metres)
  +0xa4, +0xa8 yaw / pitch range in degrees around the authored direction (class +0x34/+0x38)
  +0xac/+0xb0 vertical and +0xb4/+0xb8 horizontal screen box, +0xbc rail smoothing (docs/camera.md 4.5)
  +0xc8, +0xcc blend-in / blend-out seconds, +0xd0 priority (active-list entry, RAM ingame2)
  +0xd4..+0xdc target offset in metres (x16, used with flag 0x40000)
  +0xe4 u32 transition count, +0xe8 entries of 0x28: char[24] previous camera, f32 time, f32 in, f32 out,
        u32 forbid
NCV_ rail curve (WAD tag 9): u32 segment count n, 12 bytes pad, n x 4x4 floats, then n knots. Segment i
covers the global parameter t in [knot[i-1], knot[i]] (knot[-1] = 0) and P(t) = [t^3 t^2 t 1] . M_i,
divided by w (FUN_0014ba20 evaluates it this way; joints match within 0.02 units).
CMZ_ record (WAD param 8): header with offsets; camera names (0x20 each); per volume a vertex count and an
index count; vertices (xyzw floats); u16 edge indices; one centre per volume.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dcparse import wad_records  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def cstr(b, o, n=24):
    return b[o:o + n].split(b"\0", 1)[0].decode("latin-1")


def f32(b, o):
    return struct.unpack_from("<f", b, o)[0]


def u32(b, o):
    return struct.unpack_from("<I", b, o)[0]


def parse_cam(b):
    cam = {
        "pos": struct.unpack_from("<3f", b, 0x40),
        "fwd": struct.unpack_from("<3f", b, 0x30),
        "rail": cstr(b, 0x50),
        "flags": u32(b, 0x88),
        "modes": struct.unpack_from("<2H", b, 0x8c),
        "limits": (f32(b, 0xa4), f32(b, 0xa8)),
        "box": struct.unpack_from("<4f", b, 0xac),
        "smooth": f32(b, 0xbc),
        "blend": (f32(b, 0xc8), f32(b, 0xcc)),
        "priority": u32(b, 0xd0),
        "target": struct.unpack_from("<3f", b, 0xd4),
        "transitions": [],
    }
    n = u32(b, 0xe4) if len(b) >= 0xe8 else 0
    for i in range(n):
        o = 0xe8 + i * 0x28
        if o + 0x28 > len(b):
            break
        cam["transitions"].append((cstr(b, o), f32(b, o + 0x18), f32(b, o + 0x1c), f32(b, o + 0x20),
                                   u32(b, o + 0x24)))
    return cam


def parse_zone(b):
    offs = struct.unpack_from("<7I", b, 0x10)
    names_off, counts_off, idxcounts_off, verts_off, idx_off, centres_off = offs[1:7]
    ncam = (counts_off - names_off) // 0x20
    names = [cstr(b, names_off + 0x20 * i, 0x20) for i in range(ncam)]
    # u16 count arrays are padded to 4 bytes, so take the volume count from the centre list (one per volume)
    nvol = (u32(b, 0x0c) - centres_off) // 16
    vcounts = struct.unpack_from(f"<{nvol}H", b, counts_off)
    icounts = struct.unpack_from(f"<{nvol}H", b, idxcounts_off)
    vols, vo = [], verts_off
    for i in range(nvol):
        pts = [struct.unpack_from("<3f", b, vo + 16 * k) for k in range(vcounts[i])]
        vo += 16 * vcounts[i]
        vols.append(pts)
    centres = [struct.unpack_from("<3f", b, centres_off + 16 * i) for i in range(nvol)]
    edges, io = [], idx_off
    for i in range(nvol):
        edges.append(list(struct.unpack_from(f"<{icounts[i]}H", b, io)))
        io += 2 * icounts[i]
    return {"cameras": names, "volumes": vols, "edges": edges, "icounts": icounts, "centres": centres}


def parse_rail(b):
    n = u32(b, 0)
    segs = [[struct.unpack_from("<4f", b, 0x10 + 0x40 * i + 16 * r) for r in range(4)] for i in range(n)]
    knots = struct.unpack_from(f"<{n}f", b, 0x10 + 0x40 * n)
    pts, t0 = [], 0.0
    for i in range(n):
        for k in range(16 + (i == n - 1)):
            t = t0 + (knots[i] - t0) * k / 16
            w = (t ** 3, t * t, t, 1.0)
            v = [sum(w[r] * segs[i][r][c] for r in range(4)) for c in range(4)]
            pts.append([round(v[c] / v[3], 2) for c in range(3)])
        t0 = knots[i]
    return pts


def main():
    wad = sys.argv[1]
    name = os.path.splitext(os.path.basename(wad))[0]
    out = os.path.join(ROOT, "analysis", "cameras", name)
    os.makedirs(out, exist_ok=True)
    d = open(wad, "rb").read()
    cams, zones, rails = {}, {}, {}
    for _, tag, param, rec, body in wad_records(d):
        if not body:
            continue
        if rec.startswith("CAM_") and param == 26 and u32(body, 0) == 9 and len(body) >= 0xe4:
            cams[rec] = parse_cam(body)
        elif rec.startswith("NCV_") and tag == 9 and len(body) >= 0x10:
            try:
                rails[rec] = parse_rail(body)
            except struct.error:
                pass
        elif rec.startswith("CMZ_") and param == 8:
            try:
                zones[rec] = parse_zone(body)
            except struct.error:
                pass
    with open(os.path.join(out, "cameras.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("camera\tx\ty\tz\trail\tflags\tmode_a\tmode_b\tyaw_range\tpitch_range\tbox\trail_smooth\tblend_in\tblend_out\tpriority\t"
                "target\ttransitions\n")
        for k, c in sorted(cams.items()):
            f.write(f"{k}\t" + "\t".join(f"{v:.1f}" for v in c["pos"]) +
                    f"\t{c['rail']}\t{c['flags']:#x}\t{c['modes'][0]}\t{c['modes'][1]}\t{c['limits'][0]:g}\t"
                    f"{c['limits'][1]:g}\t{','.join(f'{v:g}' for v in c['box'])}\t{c['smooth']:g}\t{c['blend'][0]:g}\t{c['blend'][1]:g}\t{c['priority']}\t"
                    f"{','.join(f'{v:g}' for v in c['target'])}\t{len(c['transitions'])}\n")
    with open(os.path.join(out, "transitions.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("camera\tfrom\ttime\tblend_in\tblend_out\tforbid\n")
        for k, c in sorted(cams.items()):
            for t in c["transitions"]:
                f.write(f"{k}\t{t[0]}\t{t[1]:g}\t{t[2]:g}\t{t[3]:g}\t{t[4]}\n")
    with open(os.path.join(out, "zones.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("zone\tcameras\tvolumes\tvertices_per_volume\tymin\tymax\n")
        for k, z in sorted(zones.items()):
            ys = [p[1] for v in z["volumes"] for p in v] or [0]
            f.write(f"{k}\t{','.join(z['cameras'])}\t{len(z['volumes'])}\t"
                    f"{','.join(str(len(v)) for v in z['volumes'])}\t{min(ys):.0f}\t{max(ys):.0f}\n")
    # viewer overlay (analysis/levels/viewer.html loads <level>_gltf/cameras.json when present)
    gdir = os.path.join(ROOT, "analysis", "levels", name + "_gltf")
    if os.path.isdir(gdir):
        import json
        doc = {
            "cameras": [{"name": k, "pos": c["pos"], "fwd": c["fwd"], "rail": c["rail"], "priority": c["priority"],
                         "flags": c["flags"]} for k, c in sorted(cams.items())],
            "rails": rails,
            "zones": [{"name": k, "cameras": z["cameras"], "volumes": [[list(p) for p in v] for v in z["volumes"]],
                       "edges": z["edges"]} for k, z in sorted(zones.items())],
        }
        json.dump(doc, open(os.path.join(gdir, "cameras.json"), "w"))
    print(f"{name}: {len(cams)} cameras, {len(rails)} rails, {len(zones)} zones -> {out}")


if __name__ == "__main__":
    main()
