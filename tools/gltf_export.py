"""Export all MDL_ models of a WAD to glTF 2.0 (+ .bin + PNG textures). Research export.

  python tools/gltf_export.py extracted/pak/RHOD10.WAD analysis/levels/RHOD10_gltf [--exclude sky]

Decoded facts used (docs/models.md): strips, 1/16 position scale, UV/4096, vertex colour
(128 = 1.0), part material slot -> MAT -> TXR -> GFX/PAL (8-bpp enc-0 unswizzled).
Assumptions, stated in the file's `extras`: models placed at stored coordinates (node transforms not
applied); alphaMode MASK 0.5 for textures with alpha < 1 (real GS blend modes not decoded);
no lighting model (vertex colour x texture, PS2 2x modulate not applied).
"""
import argparse
import json
import math
import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from gfx_decode import TextureStore  # noqa: E402
from textured_preview import mesh_full, model_materials  # noqa: E402
from anm_decode import clips as anm_clips, decode_segment, u16  # noqa: E402
from anm_decode import clips  # noqa: E402
from rig_anim import clip_name, clip_channels, find_rigs, mat_to_trs, mesh_joints, parse_rig, rig_joint_names  # noqa: E402


def write_png_rgba(path, w, h, rgba):
    raw = b"".join(b"\0" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h))
    ch = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))  # noqa: E731
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + ch(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                           + ch(b"IDAT", zlib.compress(raw, 6)) + ch(b"IEND", b""))


def mat_mul(a_, b_):
    """4x4 row-major product a . b."""
    return [sum(a_[4 * i + k] * b_[4 * k + j] for k in range(4)) for i in range(4) for j in range(4)]


def mat_inv(m):
    """4x4 row-major inverse (Gauss-Jordan with partial pivoting)."""
    a_ = [list(m[4 * i:4 * i + 4]) + [1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]
    for c in range(4):
        p = max(range(c, 4), key=lambda r: abs(a_[r][c]))
        a_[c], a_[p] = a_[p], a_[c]
        d = a_[c][c] or 1e-12
        a_[c] = [x / d for x in a_[c]]
        for r in range(4):
            if r != c:
                f = a_[r][c]
                a_[r] = [x - f * y for x, y in zip(a_[r], a_[c])]
    return [a_[i][4 + j] for i in range(4) for j in range(4)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("out")
    ap.add_argument("--exclude", default="")
    ap.add_argument("--anm", default="", help="ANM_ record to use for rigged models that have no ANM_ reference")
    ap.add_argument("--bind-space", action="store_true",
                    help="vertices are in bind-pose model space (characters): emit real inverse bind matrices")
    ap.add_argument("--quat-conj", action="store_true",
                    help="conjugate clip quaternions (test of the row-vector convention)")
    ap.add_argument("--extra-anm", default="",
                    help="comma-separated WAD:ANM_record pairs whose clips are added to the rigged model "
                         "(character patch records such as ATLAS210.WAD:ANM_hero_WallClimb)")
    ap.add_argument("--clips", default="", help="comma-separated clip names, or all (default: the first clip)")
    ap.add_argument("--model-scale", choices=["div", "mul"], default="div",
                    help="MDL +0x48: div = quantisation factor (world = v/16/s + o, matches rig roots); "
                         "mul = earlier reading (world = v/16*s + o)")
    a = ap.parse_args()
    os.makedirs(os.path.join(a.out, "tex"), exist_ok=True)
    data = open(a.wad, "rb").read()
    seq, recs = [], {}
    for off, tag, param, n, body in wad_records(data):
        seq.append((tag, n, body))
        if tag == 1 and body:
            recs.setdefault(n, body)
    extra_anm = []
    for pair in [x for x in a.extra_anm.split(",") if x]:
        wad_, rn = pair.split(":")
        path_ = wad_ if os.path.exists(wad_) else os.path.join(os.path.dirname(a.wad), wad_)
        for _o, tag_, _p, n_, body_ in wad_records(open(path_, "rb").read()):
            if tag_ == 1 and n_ == rn and body_:
                recs.setdefault(n_, body_)
                extra_anm.append(n_)
                break
    store = TextureStore(recs)
    names = [n[4:-2] for n in recs if n.startswith("MDL_") and n.endswith("_0") and len(recs[n]) > 64]
    names = [n for n in names if not any(x and x.lower() in n.lower() for x in a.exclude.split(","))]

    bin_ = bytearray()
    g = {"asset": {"version": "2.0", "generator": "gow2 tools/gltf_export.py",
                   "extras": {"source": os.path.basename(a.wad), "scale": "1/16",
                              "assumptions": ["stored coordinates, node transforms not applied",
                                              "alphaMode MASK 0.5 where texture alpha < 1",
                                              "colour = vertex colour x texture, no PS2 lighting"]}},
         "scene": 0, "scenes": [{"nodes": []}], "nodes": [], "meshes": [], "materials": [], "textures": [],
         "images": [], "samplers": [{"wrapS": 10497, "wrapT": 10497, "magFilter": 9729, "minFilter": 9729}],
         "accessors": [], "bufferViews": [], "buffers": [], "skins": [], "animations": []}
    mat_index, report = {}, []

    def view(blob_bytes, target):
        while len(bin_) % 4:
            bin_.append(0)
        bv_ = {"buffer": 0, "byteOffset": len(bin_), "byteLength": len(blob_bytes)}
        if target:
            bv_["target"] = target
        g["bufferViews"].append(bv_)
        bin_.extend(blob_bytes)
        return len(g["bufferViews"]) - 1

    def accessor(bv, ctype, count, typ, mn=None, mx=None):
        acc = {"bufferView": bv, "componentType": ctype, "count": count, "type": typ}
        if mn is not None:
            acc["min"], acc["max"] = mn, mx
        g["accessors"].append(acc)
        return len(g["accessors"]) - 1

    def material_anim(mname):
        """Material ANM_ (docs/animation.md "Material tracks"): the ANM_ that follows MAT_x in its group.
        Track type 3 = RGB colour multiplier (slots 0-2), type 8 = UV offset (slot 0 u, slot 1 v); f32 keys."""
        anm = recs.get("ANM_" + mname[4:])
        if not anm or len(anm) < 0x30:
            return None
        try:
            ng, nt = u16(anm, 0x12), u16(anm, 0x10)
            tracks = [struct.unpack_from("<HBB", anm, 0x18 + 4 * ng + 8 * i) for i in range(nt)]
            if not tracks or any(t[0] not in (3, 8) for t in tracks):
                return None
            c = next(anm_clips(anm), None)
            if c is None:
                return None
            out = {"duration": round(struct.unpack_from("<f", anm, c + 0x14)[0], 4)}
            blk = 0
            for ttype, _b2, nsub in tracks:
                for _j in range(nsub):
                    _a, nseg, _e, _f, tab, dt = struct.unpack_from("<4HIf", anm, c + 0x60 + 16 * blk)
                    curves = {}
                    for sg in range(nseg):
                        _sl, _fl, cv, _err = decode_segment(anm, c + tab + 12 * sg, "trans")
                        for k, v in cv.items():
                            curves.setdefault(k, {}).update(v)
                    key = "color" if ttype == 3 else "uv"
                    out["dt"] = dt
                    out[key] = {str(k): [[f, round(v, 5)] for f, v in sorted(cv.items())] for k, cv in curves.items()}
                    blk += 1
            return out
        except (struct.error, IndexError, ZeroDivisionError):
            return None

    def material(mname):
        if mname in mat_index:
            return mat_index[mname]
        t = store.material_texture(mname)
        m = {"name": mname, "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 1.0},
             "doubleSided": True}
        # MAT +0x38 (docs/models.md "Material blend fields"): top byte = GS ALPHA A|B<<2|C<<4|D<<6
        # (0x44 normal, 0x48 additive, 0x42 subtractive; HIGH from names), bit 7 of low byte = textured
        # (exact on 7338 MATs), byte 1 bit 3 = blended pass (MEDIUM).
        mb = recs.get(mname)
        if mb and len(mb) == 120:
            w38, w40 = struct.unpack_from("<I", mb, 0x38)[0], struct.unpack_from("<I", mb, 0x40)[0]
            sel = w38 >> 24
            blend = {0x44: "normal", 0x48: "additive", 0x42: "subtractive"}.get(sel, f"gs{sel:02x}")
            m["extras"] = {"gsAlpha": sel, "blend": blend, "textured": bool(w38 & 0x80),
                           "blendPass": bool(w38 & 0x080000), "w38": f"{w38:08x}", "w40": f"{w40:08x}"}
            ma = material_anim(mname)
            if ma:
                m["extras"]["anim"] = ma
            if blend in ("additive", "subtractive"):
                m["alphaMode"] = "BLEND"
        if t:
            w, h, rgba = t
            fn = "tex/" + "".join(c if c.isalnum() or c in "_-." else "_" for c in mname) + ".png"
            write_png_rgba(os.path.join(a.out, fn), w, h, rgba)
            g["images"].append({"uri": fn})
            g["textures"].append({"sampler": 0, "source": len(g["images"]) - 1})
            m["pbrMetallicRoughness"]["baseColorTexture"] = {"index": len(g["textures"]) - 1}
            if "alphaMode" not in m and any(rgba[i] < 255 for i in range(3, len(rgba), 4)):
                m["alphaMode"], m["alphaCutoff"] = "MASK", 0.5
        g["materials"].append(m)
        mat_index[mname] = len(g["materials"]) - 1
        return mat_index[mname]

    # --- placement (docs/models.md "Instances"): subtype-2 ref records name a subtype-1 node, which is
    # bound to a model by the reference record that follows it in its group.
    node_model, refs, cur_node = {}, [], None
    for tag, n, body in seq:
        if tag == 2:
            cur_node = None
        if tag == 1 and body and len(body) >= 4:
            tw = struct.unpack_from("<I", body)[0]
            if tw == 0x00010001:
                cur_node = n
            elif tw == 0x00020001 and len(body) >= 0x5c:
                tgt = body[4:28].split(b"\0", 1)[0].decode("latin-1")
                R = struct.unpack_from("<9f", body, 0x20)
                W = struct.unpack_from("<3f", body, 0x50)
                refs.append((n, tgt, R, W))
        elif tag == 1 and not body and n.startswith("MDL_") and cur_node:
            node_model.setdefault(cur_node, n[4:])
    # per-model transform from the 88-byte MDL_<n> record (docs/models.md): +0x38 offset vec3,
    # +0x48 f32 scale; matches the RAM model object (+0x08 offset, +0x90 scale). +0x50 u32 is
    # exported raw as extras.mdl50 (not a sky flag: also set on ropes, mirrors, actors; R14).
    model_xf = {}
    for tag, n, body in seq:
        if tag == 1 and n.startswith("MDL_") and len(body) == 88:
            off = struct.unpack_from("<3f", body, 0x38)
            sc = struct.unpack_from("<f", body, 0x48)[0] or 1.0
            if a.model_scale == "div":
                sc = 1.0 / sc
            model_xf[n[4:]] = (off, sc, struct.unpack_from("<I", body, 0x50)[0])
    placed_models = set()
    # go nodes (docs/animation.md "go node placement"): a 104-byte tag-1 record with header u16 1, u16 3
    # names a rig group (GroupStart, rig record, ANM_/MDL_/ESC_/FXC_ refs). +0x20 nine rotation floats,
    # +0x44 translation, +0x50 vec3 = world bounds centre (matches the placed geometry; RAM copy at
    # 0x19bcd00 in the ingame1 dump for gofirstroom).
    go_xf = {}
    for i, (tag, n, body) in enumerate(seq):
        if tag == 1 and body and len(body) == 104 and struct.unpack_from("<HH", body) == (1, 3):
            for tag2, n2, b2 in seq[i + 1:i + 16]:
                if tag2 == 1 and b2 and n2.startswith("go"):
                    break
                if tag2 == 1 and not b2 and n2.startswith("MDL_"):
                    go_xf.setdefault(n2[4:], (n, struct.unpack_from("<9f", body, 0x20),
                                              struct.unpack_from("<3f", body, 0x44)))
                    break
    instances = []
    for n, tgt, R, W in refs:
        m = node_model.get(tgt)
        if m:
            instances.append((n, m, R, W))
            placed_models.add(m)
    mesh_of = {}
    # Rigged models (docs/animation.md): vertices are joint-local; a multi-joint rig or an ANM_ makes
    # the model a glTF skin (weight 1 per vertex on its GIF-tag palette joint, identity inverse binds).
    rigs = find_rigs(seq)
    rig_info = {}
    jnames = {}
    anm_cache = {}

    def floats(vals, typ, mn=False):
        flat = [x for v in vals for x in (v if isinstance(v, (list, tuple)) else [v])]
        return accessor(view(struct.pack(f"<{len(flat)}f", *flat), None), 5126, len(vals), typ,
                        [min(vals)] if mn else None, [max(vals)] if mn else None)

    def place(node_name, model, matrix, extras, plain=None):
        # rigged models: the rig root joint already carries the MDL record offset/scale (docs/animation.md),
        # so their placement node gets only the instance transform (`plain`)
        if model in rig_info:
            matrix = plain or [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
        if model not in rig_info:
            g["nodes"].append({"name": node_name, "mesh": mesh_of[model], "matrix": matrix, "extras": extras})
            g["scenes"][0]["nodes"].append(len(g["nodes"]) - 1)
            return
        parents, rmats, anm_name = rig_info[model]
        g["nodes"].append({"name": node_name, "matrix": matrix, "children": [], "extras": extras})
        root = len(g["nodes"]) - 1
        g["scenes"][0]["nodes"].append(root)
        jn, trs = [], []
        for jt, m_ in enumerate(rmats):
            t_, q_, s_ = mat_to_trs(m_)
            trs.append((t_, q_, s_))
            jname = jnames.get(model, [])[jt] if jt < len(jnames.get(model, [])) else ""
            g["nodes"].append({"name": f"{node_name}:{jname or 'j%d' % jt}", "translation": t_, "rotation": q_,
                               "scale": s_})
            jn.append(len(g["nodes"]) - 1)
        for jt, par in enumerate(parents):
            host = g["nodes"][jn[par]] if 0 <= par < len(jn) else g["nodes"][root]
            host.setdefault("children", []).append(jn[jt])
        if a.bind_space:
            # Character meshes (Kratos) store vertices in bind-pose model space, not joint-local:
            # inverse bind = inverse of each joint's world bind matrix (row-vector W = L . W_parent;
            # its row-major inverse is the column-major glTF inverse bind matrix).
            world = []
            for jt, m_ in enumerate(rmats):
                par = parents[jt]
                world.append(mat_mul(m_, world[par]) if 0 <= par < jt else list(m_))
            ibm = floats([mat_inv(w_) for w_ in world], "MAT4")
        else:
            ibm = floats([[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]] * len(jn), "MAT4")
        g["skins"].append({"joints": jn, "inverseBindMatrices": ibm, "skeleton": root})
        g["nodes"].append({"name": node_name + ":mesh", "mesh": mesh_of[model], "skin": len(g["skins"]) - 1})
        g["scenes"][0]["nodes"].append(len(g["nodes"]) - 1)
        anm_name = anm_name or a.anm or None
        if not anm_name or anm_name not in recs:
            return
        sources = [anm_name] + [n_ for n_ in extra_anm if n_ in recs]
        seen = set()
        for src in sources:
            if a.clips == "all" or src != anm_name:
                wanted = []
                for c_ in clips(recs[src]):
                    nm_ = clip_name(recs[src], c_)
                    if nm_ and nm_ not in seen:
                        seen.add(nm_)
                        wanted.append(nm_)
            else:
                wanted = [c_.strip() for c_ in a.clips.split(",") if c_.strip()] or [None]
            for clip in wanted:
                key = (src, clip)
                if key not in anm_cache:
                    try:
                        anm_cache[key] = clip_channels(recs[src], len(jn), clip)
                    except (struct.error, IndexError, ZeroDivisionError):
                        anm_cache[key] = None
                if anm_cache[key] and anm_cache[key][2]:
                    add_animation(node_name, clip or src, anm_cache[key], jn, trs)

    def add_animation(node_name, anim_name, cc, jn, trs):
        dt, dur, chans = cc
        samplers, channels = [], []
        for jt, kinds in chans.items():
            t0, q0, s0 = trs[jt]
            for kind, comps in kinds.items():
                frames = sorted({f for cv in comps.values() for f in cv})
                if not frames:
                    continue
                qd = [-q0[0], -q0[1], -q0[2], q0[3]] if a.quat_conj else q0  # bind in the clip's convention
                cur = {"rot": [c * 16384 for c in qd], "trans": list(t0), "scale": list(s0)}[kind]
                vals = []
                for f in frames:
                    for c_, cv in comps.items():
                        if f in cv and c_ < len(cur):
                            cur[c_] = cv[f]
                    v = list(cur)
                    if kind == "rot":
                        nrm = math.sqrt(sum(x * x for x in v)) or 1.0
                        v = [x / nrm for x in v]
                        if a.quat_conj:
                            v = [-v[0], -v[1], -v[2], v[3]]
                    vals.append(v)
                inp = floats([f * dt for f in frames], "SCALAR", mn=True)
                out = floats(vals, "VEC4" if kind == "rot" else "VEC3")
                samplers.append({"input": inp, "output": out, "interpolation": "LINEAR"})
                path = {"rot": "rotation", "trans": "translation", "scale": "scale"}[kind]
                channels.append({"sampler": len(samplers) - 1, "target": {"node": jn[jt], "path": path}})
        if channels:
            g["animations"].append({"name": f"{node_name}:{anim_name}", "samplers": samplers,
                                    "channels": channels, "extras": {"duration": dur}})

    for name in names:
        try:
            mats = model_materials(seq, name)
            J = None
            if name in rigs:
                parents, rmats = parse_rig(rigs[name][0])
                jnames[name] = rig_joint_names(rigs[name][0])
                if len(parents) > 1 or rigs[name][1]:
                    V, UV, COL, J, T = mesh_joints(recs[f"MDL_{name}_0"], 4096.0)
                    if max(J, default=0) >= len(parents):
                        J = None
                    else:
                        rig_info[name] = (parents, rmats, rigs[name][1])
            if J is None:
                V, UV, COL, T = mesh_full(recs[f"MDL_{name}_0"], 4096.0)
        except Exception as e:  # noqa: BLE001
            report.append({"model": name, "error": repr(e)})
            continue
        if not T:
            continue
        pos = [(x / 16, y / 16, z / 16) for x, y, z in V]
        pb = b"".join(struct.pack("<3f", *p) for p in pos)
        ub = b"".join(struct.pack("<2f", u, v) for u, v in UV)
        cb = b"".join(struct.pack("<4f", *(min(c[q], 128) / 128 for q in range(4))) for c in COL)
        pa = accessor(view(pb, 34962), 5126, len(pos), "VEC3",
                      [min(p[i] for p in pos) for i in range(3)], [max(p[i] for p in pos) for i in range(3)])
        ua = accessor(view(ub, 34962), 5126, len(UV), "VEC2")
        ca = accessor(view(cb, 34962), 5126, len(COL), "VEC4")
        attrs = {"POSITION": pa, "TEXCOORD_0": ua, "COLOR_0": ca}
        if J is not None:
            attrs["JOINTS_0"] = accessor(view(b"".join(struct.pack("<4H", j, 0, 0, 0) for j in J), 34962),
                                         5123, len(J), "VEC4")
            attrs["WEIGHTS_0"] = accessor(view(struct.pack("<4f", 1, 0, 0, 0) * len(J), 34962),
                                          5126, len(J), "VEC4")
        prims = []
        for slot in sorted({t[3] for t in T}):
            idx = [i for t in T if t[3] == slot for i in t[:3]]
            ia = accessor(view(struct.pack(f"<{len(idx)}I", *idx), 34963), 5125, len(idx), "SCALAR")
            prim = {"attributes": dict(attrs), "indices": ia}
            if slot < len(mats):
                prim["material"] = material(mats[slot])
            prims.append(prim)
        g["meshes"].append({"name": name, "primitives": prims})
        mesh_of[name] = len(g["meshes"]) - 1
        off, sc, flag = model_xf.get(name, ((0.0, 0.0, 0.0), 1.0, 0))
        if name not in placed_models and name in go_xf and name in rig_info:
            # rigged model placed by its go node: world = rig-local . R + W (row vectors). HIGH: the go
            # record's +0x50 bounds centre lies within ~25 units of the transformed root joint for most
            # RHOD10 rigs (DoorSparkle 4, chunks2 20, balcony 24; rotated firstRoom 104 vs 6711 with the
            # transposed convention). Unrigged models already sit at world coordinates through their MDL
            # record offset (ColossusSheet: adding the go transform would put it 6971 units off), so they
            # keep the model-record placement below.
            gn, R, W = go_xf[name]
            plain = [R[0], R[1], R[2], 0.0, R[3], R[4], R[5], 0.0, R[6], R[7], R[8], 0.0, W[0], W[1], W[2], 1.0]
            place(name, name, plain, {"placement": "go", "go": gn, "mdl50": flag}, plain)
        elif name not in placed_models:  # not instanced by a ref record: model record transform only
            place(name, name, [sc, 0, 0, 0, 0, sc, 0, 0, 0, 0, sc, 0, off[0], off[1], off[2], 1],
                  {"placement": "model-record", "mdl50": flag})
        report.append({"model": name, "verts": len(V), "tris": len(T), "materials": mats,
                       "placement": "ref instances" if name in placed_models else "stored"})
    for n, m, R, W in instances:
        if m not in mesh_of:
            continue
        # world = (s*v + o) . R + W with R rows at +0x20, row vectors (docs/models.md: the built ref object
        # in RAM keeps the rows as a row-major 4x4 and the VU programs compute row3 + x.row0 + y.row1 + z.row2).
        # ~~world = R . (s*v + o) + W~~ (transposed; corrected 2026-10-02). glTF matrices are column-major,
        # so the column-major array of a row-vector matrix is its rows in order.
        (o, s, _f) = model_xf.get(m, ((0.0, 0.0, 0.0), 1.0, 0))
        t = [o[0] * R[0] + o[1] * R[3] + o[2] * R[6] + W[0], o[0] * R[1] + o[1] * R[4] + o[2] * R[7] + W[1],
             o[0] * R[2] + o[1] * R[5] + o[2] * R[8] + W[2]]
        mat = [R[0] * s, R[1] * s, R[2] * s, 0.0, R[3] * s, R[4] * s, R[5] * s, 0.0,
               R[6] * s, R[7] * s, R[8] * s, 0.0, t[0], t[1], t[2], 1.0]
        plain = [R[0], R[1], R[2], 0.0, R[3], R[4], R[5], 0.0, R[6], R[7], R[8], 0.0, W[0], W[1], W[2], 1.0]
        place(n, m, mat, {"placement": "ref"}, plain)
    report.append({"instances": len(instances), "refs": len(refs), "node_models": len(node_model), "go_placed": len(go_xf),
                   "rigged_models": len(rig_info), "animations": len(g["animations"])})
    for k_ in ("skins", "animations"):
        if not g[k_]:
            del g[k_]
    # Fog scripts (docs/rendering.md §3): SCP_ records of class SCR_Fog / SCR_LayeredFog. Param block
    # P = record+0x24: P+0x10 colour RGB + density, P+0x20/+0x30 extra layer colours, P+0x4c u32
    # distance (camera Z range = 2x), P+0x50/+0x54/+0x58 layer scroll u/v/rotation, P+0x60 material,
    # P+0x78 blend mode (0 lerp, 1 add, 2 sub), P+0x7c CLUT alpha (-1 = default).
    fogs, seen_fog = [], set()
    for tag, n, body in seq:
        if tag == 1 and n.startswith("SCP_") and len(body) >= 0xa8 and n not in seen_fog:
            cls = body[4:0x1c].split(b"\0", 1)[0].decode("latin-1")
            if cls not in ("SCR_Fog", "SCR_LayeredFog"):
                continue
            seen_fog.add(n)
            P = 0x24
            f = lambda o: struct.unpack_from("<f", body, P + o)[0]  # noqa: E731
            fogs.append({"name": n[4:], "class": cls,
                         "color": [round(f(0x10 + 4 * i), 4) for i in range(3)], "density": round(f(0x1c), 4),
                         "layers": [[round(f(o + 4 * i), 4) for i in range(4)] for o in (0x20, 0x30)],
                         "distance": struct.unpack_from("<I", body, P + 0x4c)[0],
                         "scroll": [round(f(0x50), 4), round(f(0x54), 4), round(f(0x58), 4)],
                         "material": body[P + 0x60:P + 0x78].split(b"\0", 1)[0].decode("latin-1"),
                         "mode": struct.unpack_from("<i", body, P + 0x78)[0],
                         "clutAlpha": struct.unpack_from("<i", body, P + 0x7c)[0]})
    g["asset"]["extras"]["fog"] = fogs
    open(os.path.join(a.out, "level.bin"), "wb").write(bin_)
    g["buffers"].append({"uri": "level.bin", "byteLength": len(bin_)})
    json.dump(g, open(os.path.join(a.out, "level.gltf"), "w"))
    json.dump(report, open(os.path.join(a.out, "report.json"), "w"), indent=1)
    print(f"{len(g['meshes'])} meshes, {len(g['materials'])} materials, {len(g['images'])} textures, "
          f"{len(bin_) / 1e6:.1f} MB; errors {sum('error' in r for r in report)}")


if __name__ == "__main__":
    main()
