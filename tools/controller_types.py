"""List creature controller types: gcc vtables (.data/.rodata) whose slot 6 returns a type-name string.

Shape established from the hero (vtable 0x002effc8) and Rhodes-soldier (0x002eff70) controllers,
both observed in RAM (docs/runtime-validation.md):
  slot 1  +0x0c  destructor
  slot 2  +0x14  create(controller, spawner): builds the Character (vtable 0x2f1440) etc.
  slot 5  +0x2c  factory: allocates a controller from the current WAD heap and constructs it
  slot 6  +0x34  returns the type name ("Player", "Enemy1")
  slot 9  +0x4c  returns hash of an HFSM name ("hfsmPlayer", "hfsmEnemy1")
Writes analysis/controller_types.tsv.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, resolve_pairs  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    img = Image()
    tlo, thi = img.text
    in_text = lambda x: tlo <= x < thi  # noqa: E731

    def first_string(fn, length, prefix=""):
        if not in_text(fn):
            return None
        for a, w, m, ops, res in resolve_pairs(img, fn, fn + length):
            if res is not None:
                s = img.cstr(res)
                if s and s.startswith(prefix):
                    return s
        return None

    lo, hi = img.sections[".data"][0], img.sections[".rodata"][1]  # vtables live in .rodata
    rows = []
    for vt in range(lo, hi - 0x50, 8):
        if img.word(vt) or img.word(vt + 4) or img.word(vt + 8):
            continue
        s1, s2, s5, s6, s9 = (img.word(vt + o) for o in (0xC, 0x14, 0x2C, 0x34, 0x4C))
        if not (in_text(s1) and in_text(s2) and in_text(s6)):
            continue
        name = first_string(s6, 16)
        if not name:
            continue
        rows.append((vt, name, first_string(s9, 64, "hfsm") or "", s1, s2, s5, s9))
    out = os.path.join(ROOT, "analysis", "controller_types.tsv")
    with open(out, "w", encoding="utf-8", newline="") as f:
        f.write("vtable\ttype_name\thfsm\tslot1_dtor\tslot2_create\tslot5_factory\tslot9_hfsm_fn\n")
        for r in rows:
            f.write(f"{r[0]:08x}\t{r[1]}\t{r[2]}\t{r[3]:08x}\t{r[4]:08x}\t{r[5]:08x}\t{r[6]:08x}\n")
    for r in rows:
        print(f"{r[0]:08x}  {r[1]:28s} {r[2] or '-':28s} create={r[4]:08x}")
    print(f"{len(rows)} controller types -> {out}")


if __name__ == "__main__":
    main()
