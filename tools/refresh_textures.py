"""Rewrite the PNG textures of every exported level/character with the corrected palette order (gfx_decode csm1 fix)."""
import glob, os, sys
sys.path.insert(0, "tools")
from dcparse import wad_records
from gfx_decode import TextureStore
from gltf_export import write_png_rgba

def clean(n): return "".join(c if c.isalnum() or c in "_-." else "_" for c in n)
total = changed = 0
for d in sorted(glob.glob("analysis/levels/*_gltf") + glob.glob("analysis/levels/*_fx")):
    name = os.path.basename(d).rsplit("_", 1)[0]
    wad = f"extracted/pak/{name}.WAD"
    tex = os.path.join(d, "tex")
    if not os.path.exists(wad) or not os.path.isdir(tex):
        continue
    recs = {}
    for _o, tag, _p, n, body in wad_records(open(wad, "rb").read()):
        if tag == 1 and body: recs.setdefault(n, body)
    store = TextureStore(recs)
    for n in recs:
        if not n.startswith("MAT_"): continue
        for fn in (clean(n) + ".png", "ptc_" + clean(n) + ".png"):
            p = os.path.join(tex, fn)
            if os.path.exists(p):
                t = store.material_texture(n)
                if t:
                    before = open(p, "rb").read()
                    write_png_rgba(p, *t)
                    total += 1
                    changed += before != open(p, "rb").read()
    print(d, "done", flush=True)
print(total, "textures rewritten,", changed, "changed")
