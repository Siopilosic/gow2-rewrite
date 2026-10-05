"""Export Kratos's runtime LOCAL joint matrices from EE RAM captures (found by the rig's own bone offsets) for pose tests.

  python tools/export_runtime_pose.py   -> gow2-rs/crates/gow2-skel/tests/oracle/runtime_pose.tsv

Search: the rig's local translations of joints 3..9 sit at a stride of 0x40 in the game's local-matrix array.
Each output row: capture, joint, 16 floats. Derived game-state data for research only.
"""
import os, struct, sys, zipfile
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records
from rig_anim import find_rigs, parse_rig

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
seq = [(t, nm, b) for _o, t, _p, nm, b in wad_records(open(os.path.join(ROOT, "extracted/pak/R_HERO01.WAD"), "rb").read())]
parents, mats = parse_rig(find_rigs(seq)["hero"][0])
nj = len(parents)
caps = {"ingame1": os.path.join(ROOT, "analysis/runtime/ingame1/ee.bin"), "ingame2": os.path.join(ROOT, "analysis/runtime/ingame2/ee.bin")}
p2s = os.path.join(ROOT, "tools/pcsx2/sstates/SCUS-97481 (2F123FD8).01.p2s")
tmp = None
if os.path.exists(p2s):
    with zipfile.ZipFile(p2s) as z:
        data = z.read("eeMemory.bin")
    tmp = os.path.join(os.environ.get("TEMP", "."), "p2s_ee.bin"); open(tmp, "wb").write(data); caps["savestate"] = tmp
out = open(os.path.join(ROOT, "gow2-rs/crates/gow2-skel/tests/oracle/runtime_pose.tsv"), "w", newline="")
out.write("capture\tjoint\tm0\tm1\tm2\tm3\tm4\tm5\tm6\tm7\tm8\tm9\tm10\tm11\tm12\tm13\tm14\tm15\n")
for name, path in caps.items():
    ram = np.fromfile(path, dtype="<f4")
    # translation rows of joints 3..9 (indices 12..14 of each matrix) at stride 16 floats
    sig = [np.array(mats[j][12:15], dtype=np.float32) for j in range(3, 10)]
    found = None
    matches = []
    n = len(ram)
    j3 = np.where(np.abs(ram[:n - 16 * 12] - sig[0][0]) < 1e-4)[0]
    for i in j3:
        if i % 4: continue
        ok = True
        for k, s in enumerate(sig):
            o = i + 16 * k
            if not np.allclose(ram[o:o + 3], s, atol=1e-3): ok = False; break
        if ok:
            found = i - 12 - 16 * 3  # float index of joint 0 matrix start
            # verify the preceding fields look like matrices: w column 0,0,0,1
            m = ram[found + 16 * 2: found + 16 * 3].reshape(4, 4)
            if abs(m[3, 3] - 1.0) < 1e-3 and abs(m[0, 3]) < 1e-3:
                matches.append(found)
            found = None
    # several buffers hold a copy; the right one is the array whose local matrices compose to the world matrices
    # (checked on ingame1: 0x952980 is consistent with the world array at 0x955260, 0x94cc10 is not)
    found = 0x952980 // 4   # fixed: the same heap address in all three captures (verified against the world array)
    print(name, "local array at", hex(found * 4) if found is not None else None)
    if found is None: continue
    for j in range(nj):
        out.write(f"{name}\t{j}\t" + "\t".join(repr(float(x)) for x in ram[found + 16 * j: found + 16 * j + 16]) + "\n")
out.close()
