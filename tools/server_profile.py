"""Per-server profile built from the factory in Master_Server vtable slot 19 (0x00185be0).

For every server id the factory allocates `size` bytes and either calls an out-of-line
constructor or constructs inline with a literal vtable. This script records that table
(transcribed from the factory's decompilation, see analysis/disasm & docs) and, for
out-of-line constructors, finds the vtables the constructor installs (in code order;
the last one is the most-derived class).

  python server_profile.py > analysis/servers.tsv
"""
import csv
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from eedis import Image, resolve_pairs  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# id: (name, alloc size, ctor or None, inline vtable or None) -- from FUN_00185be0 switch
FACTORY = {
    0x01: ("GOServer", 0x11C, 0x00142888, None),
    0x02: ("renPrimMaster", 0x16C, 0x0016D740, None),
    0x03: ("AnimServer", 0x4DC, None, 0x002F5E28),
    0x04: ("ScriptServer", 0xD8, 0x0013FE48, None),
    0x05: ("Master_Server", 0x34, None, 0x002F4808),
    0x06: ("LightServer", 0xD8, None, 0x002F26B0),
    0x07: ("TextureServer", 0xD8, None, 0x002F2930),
    0x08: ("MatServer", 0xD8, 0x00162C08, None),
    0x09: ("CameraServer", 0xD8, None, 0x002F2AD0),
    0x0A: ("ProServer", 0x30, None, 0x002F1FA0),
    0x0B: ("EpiServer", 0x2C, None, 0x002F48E8),
    0x0C: ("GfxClutServer", 0xD8, 0x0015B328, None),
    0x0D: ("renMasterSvr", 0x4C, 0x00160A68, None),
    0x10: ("CollisionServer", 0xD8, 0x0011F240, None),
    0x12: ("WaypointServer", 0xD8, 0x0018F908, None),
    0x13: ("EvtServer", 0x3834, 0x00121788, None),
    0x14: ("BhvrServer", 0xD8, 0x0011E280, None),
    0x15: ("SoundServer", 0xD8, 0x00180188, None),
    0x16: ("WadServer", 0xE0, None, 0x002F6110),
    0x19: ("EffectsServer", 0xD8, 0x00138518, None),
    0x1B: ("renFlashServer", 0xD8, 0x00158D90, None),
}

# Render servers are built by renMasterSvr's own factory (vtable slot 19 = 0x00160bc8);
# its id 0x1b case duplicates Master's. All are 0xd8 bytes.
REN_FACTORY = {
    0x0F: ("renModelServer", 0xD8, 0x001654D0, None),
    0x11: ("renParticleSvr", 0xD8, 0x0016BF58, None),
    0x17: ("renEEPrimSvr", 0xD8, 0x0014FB90, None),
    0x20: ("renShadowServer", 0xD8, 0x00173310, None),
}
ALL = {**FACTORY, **REN_FACTORY}


def func_sizes():
    out = {}
    p = os.path.join(ROOT, "analysis", "exports", "pass2", "functions.tsv")
    for r in csv.DictReader(open(p, encoding="utf-8-sig"), delimiter="\t"):
        out[int(r["addr"], 16)] = int(r["size"])
    return out


def vtables_set():
    p = os.path.join(ROOT, "analysis", "vtables.tsv")
    return {int(r["vtable"], 16): r for r in csv.DictReader(open(p, encoding="utf-8-sig"), delimiter="\t")}


def vtables_in(img, start, size, vts):
    """All vtables referenced by the constructor, in code order (informational chain)."""
    seen = []
    for a, w, m, ops, res in resolve_pairs(img, start, start + size):
        if res in vts and res not in seen:
            seen.append(res)
    return seen


def vptr_stores(img, start, size, vts):
    """Vtables the constructor stores into the vptr slot at +0x20, in code order.

    Only `sw <reg>, 0x20(<base>)` within 16 instructions of the vtable address being formed
    counts. The renPrimMaster ctor also builds helper objects whose vptrs live at +0x60
    (0x002f4520, 0x002f44e8); the first version of this tool took the last referenced vtable as
    the final one and got renPrimMaster wrong (corrected by runtime evidence,
    analysis/runtime/title-report)."""
    insns = list(resolve_pairs(img, start, start + size))
    seen = []
    for i, (a, w, m, ops, res) in enumerate(insns):
        if res not in vts or res in seen:
            continue
        reg = ops.split(",")[0].strip()
        for a2, w2, m2, ops2, _ in insns[i + 1:i + 17]:
            if m2 == "sw" and ops2.split(",")[0].strip() == reg and ops2.split(",")[1].strip().startswith("0x20("):
                seen.append(res)
                break
    return seen


def main():
    img = Image()
    sizes = func_sizes()
    vts = vtables_set()
    print("id\tname\tsize\tctor\tvtables_installed\tfinal_vtable\tfinal_slots\tslot_fns")
    for sid, (name, size, ctor, inline) in sorted(ALL.items()):
        chain = vtables_in(img, ctor, sizes.get(ctor, 0x400), vts) if ctor else [0x002F6A48, inline]
        stored = vptr_stores(img, ctor, sizes.get(ctor, 0x400), vts) if ctor else chain
        final = (stored or chain or [None])[-1]
        row = vts.get(final, {}) if final else {}
        print(f"{sid:#04x}\t{name}\t{size:#x}\t{(f'{ctor:08x}' if ctor else 'inline')}\t"
              f"{','.join(f'{v:08x}' for v in chain)}\t{(f'{final:08x}' if final else '?')}\t"
              f"{row.get('slots', '?')}\t{row.get('slot_fns', '')}")


if __name__ == "__main__":
    main()
