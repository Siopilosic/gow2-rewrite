"""List the native script classes registered with the ScriptServer (docs/scripting.md).

  python tools/scr_registry.py [--pass pass7] [--out analysis/scr_registry.tsv]

Two registration forms exist in the decompilation:
  FUN_001400c8("SCR_x", fn)              one callback, stored in descriptor slot 3 (+0xc)
  d = FUN_0013ec20(); d[k] = fn ...; FUN_00140088("SCR_x", d)   a full 7-slot descriptor
Names built at run time ("SCR_%s" in renMaster's constructor) are not literal and are listed in
docs/rendering.md instead.
"""
import argparse
import glob
import os
import re

ROOT = os.path.join(os.path.dirname(__file__), "..")
HDR = re.compile(r"^// ---- (\S+) @ ([0-9a-f]{8})")
ONE = re.compile(r'FUN_001400c8\("([^"]+)",\s*&?(\w+)')
SLOT = re.compile(r"puVar\d+\[(\d)\]\s*=\s*(?:\(\w+\)\s*)?&?(\w+)\s*;")


def addr_of(names, ident):
    """Address (8 hex digits) of a FUN_/LAB_ label or a named function; None for other identifiers."""
    m = re.match(r"^(?:FUN|LAB)_([0-9a-f]{8})$", ident)
    return m.group(1) if m else names.get(ident)
FULL = re.compile(r'FUN_00140088\("([^"]+)"')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pass", dest="pass_", default="pass7")
    ap.add_argument("--out", default=os.path.join(ROOT, "analysis", "scr_registry.tsv"))
    a = ap.parse_args()
    rows = []
    names = {}
    for line in open(os.path.join(ROOT, "analysis", "exports", a.pass_, "functions.tsv"), encoding="utf-8"):
        f = line.split("	")
        if len(f) > 1 and not line.startswith(("#", "addr")):
            names[f[1]] = f[0]
    for f in sorted(glob.glob(os.path.join(ROOT, "analysis", "exports", a.pass_, "decomp", "*.c"))):
        func = None
        slots = {}
        for line in open(f, encoding="utf-8", errors="replace"):
            m = HDR.match(line)
            if m:
                func, slots = m.group(2), {}
                continue
            for name, fn in ONE.findall(line):
                if addr_of(names, fn):
                    rows.append((name, func, {3: addr_of(names, fn)}))
            for k, fn in SLOT.findall(line):
                if addr_of(names, fn):
                    slots[int(k)] = addr_of(names, fn)
            for name in FULL.findall(line):
                rows.append((name, func, dict(slots)))
    with open(a.out, "w", encoding="utf-8") as fh:
        fh.write("name\tregistered_by\tslot1\tslot3\tslot6\n")
        for name, by, s in sorted(rows):
            fh.write(f"{name}\t{by}\t{s.get(1, '')}\t{s.get(3, '')}\t{s.get(6, '')}\n")
    print(len(rows), "registrations from", len({r[1] for r in rows}), "functions")


if __name__ == "__main__":
    main()
