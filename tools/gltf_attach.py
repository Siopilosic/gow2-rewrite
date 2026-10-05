"""Attach meshes from one glTF export to joints of another (e.g. Kratos's blades to his hands).

  python tools/gltf_attach.py <character_dir> <prop_dir> <prop_mesh> <joint>[,<joint>...] [--out DIR]

Both directories are tools/gltf_export.py outputs (level.gltf + level.bin + tex/). The prop's mesh is
added once and instanced as a child node of each named joint node (node names "<model>:<joint>").
The prop keeps its stored coordinates, so it sits at the joint origin in the joint's frame; whether that
matches the game's attachment is checked visually (docs/animation.md, Kratos weapons).
"""
import argparse
import json
import os
import shutil


def load(d):
    g = json.load(open(os.path.join(d, "level.gltf"), encoding="utf-8"))
    b = open(os.path.join(d, "level.bin"), "rb").read()
    return g, b


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("character")
    ap.add_argument("prop")
    ap.add_argument("mesh", help="prop mesh name (glTF mesh name, e.g. MAIblade)")
    ap.add_argument("joints", help="comma-separated joint names, e.g. lWeapIH,rWeapIH")
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    out = a.out or a.character
    g, b = load(a.character)
    p, pb = load(a.prop)
    mi = next(i for i, m in enumerate(p["meshes"]) if m.get("name", "").endswith(a.mesh))

    # append the prop's binary after the character's, 4-byte aligned
    pad = (-len(b)) % 4
    base = len(b) + pad
    b = b + bytes(pad) + pb
    g["buffers"][0]["byteLength"] = len(b)
    bv0, ac0, im0, tx0, mt0, sm0 = (len(g.get(k, [])) for k in
                                   ("bufferViews", "accessors", "images", "textures", "materials", "samplers"))
    for bv in p.get("bufferViews", []):
        bv = dict(bv, buffer=0, byteOffset=bv.get("byteOffset", 0) + base)
        g.setdefault("bufferViews", []).append(bv)
    for ac in p.get("accessors", []):
        g.setdefault("accessors", []).append(dict(ac, bufferView=ac["bufferView"] + bv0))
    for im in p.get("images", []):
        im = dict(im)
        if "uri" in im:
            src = os.path.join(a.prop, im["uri"])
            im["uri"] = "tex/prop_" + os.path.basename(im["uri"])
            os.makedirs(os.path.join(out, "tex"), exist_ok=True)
            shutil.copy(src, os.path.join(out, im["uri"]))
        if "bufferView" in im:
            im["bufferView"] += bv0
        g.setdefault("images", []).append(im)
    for sm in p.get("samplers", []):
        g.setdefault("samplers", []).append(sm)
    for tx in p.get("textures", []):
        tx = dict(tx, source=tx["source"] + im0)
        if "sampler" in tx:
            tx["sampler"] += sm0
        g.setdefault("textures", []).append(tx)
    for mt in p.get("materials", []):
        mt = json.loads(json.dumps(mt))
        pbr = mt.get("pbrMetallicRoughness", {})
        if "baseColorTexture" in pbr:
            pbr["baseColorTexture"]["index"] += tx0
        g.setdefault("materials", []).append(mt)
    mesh = json.loads(json.dumps(p["meshes"][mi]))
    for prim in mesh["primitives"]:
        prim["attributes"] = {k: v + ac0 for k, v in prim["attributes"].items()}
        if "indices" in prim:
            prim["indices"] += ac0
        if "material" in prim:
            prim["material"] += mt0
        for k in ("JOINTS_0", "WEIGHTS_0"):
            prim["attributes"].pop(k, None)
    g["meshes"].append(mesh)
    new_mesh = len(g["meshes"]) - 1

    # a prop exported as a one-joint skin keeps its transform (e.g. the blade's 1/64 quantisation scale,
    # docs/models.md +0x48) on that joint: copy the joint's TRS to the attached node
    trs = {}
    pnode = next((n for n in p["nodes"] if n.get("mesh") == mi), None)
    if pnode is not None and "skin" in pnode and len(p["skins"][pnode["skin"]]["joints"]) == 1:
        jnode = p["nodes"][p["skins"][pnode["skin"]]["joints"][0]]
        trs = {k: jnode[k] for k in ("translation", "rotation", "scale", "matrix") if k in jnode}

    names = {n.get("name", ""): i for i, n in enumerate(g["nodes"])}
    for jn in a.joints.split(","):
        ji = next((i for nm, i in names.items() if nm.endswith(":" + jn)), None)
        if ji is None:
            raise SystemExit(f"joint {jn} not found")
        g["nodes"].append(dict({"name": f"{a.mesh}@{jn}", "mesh": new_mesh}, **trs))
        g["nodes"][ji].setdefault("children", []).append(len(g["nodes"]) - 1)

    if out != a.character:
        shutil.copytree(os.path.join(a.character, "tex"), os.path.join(out, "tex"), dirs_exist_ok=True)
    os.makedirs(out, exist_ok=True)
    open(os.path.join(out, "level.bin"), "wb").write(b)
    json.dump(g, open(os.path.join(out, "level.gltf"), "w", encoding="utf-8"))
    print(f"attached {a.mesh} to {a.joints}; {len(g['meshes'])} meshes")


if __name__ == "__main__":
    main()
