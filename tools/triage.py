"""Automatic triage of trivial game functions for the Phase-1 coverage (tools/coverage.py).

  python tools/triage.py [--pass pass7] [--out analysis/triage.tsv] [--show N]

A function is classified "thunk" (trivial accessor, setter or wrapper; MEDIUM, automatic) when its
Ghidra decompilation, below LIB_START, has:
  - no control flow (no if / while / for / do / goto / switch / ternary), and
  - either no calls and at most MAX_STMT statements (accessor, setter, constant return, empty stub),
  - or exactly one call and at most 3 statements (forwarding wrapper).
Such a body is its own complete description: the decompiler output reproduces the behaviour.
A function is classified "dead" (MEDIUM, automatic) when Ghidra finds no caller, no 32-bit word in the
loaded ELF image equals its address (vtables, tables, initialisers) and no lui + addiu/ori pair in the
code builds its address. GCC 2.9x keeps unreferenced functions (no section garbage collection), so these
are emitted but unreachable; most are unused copy-assignment operators, constructors and wrappers.
Explicit entries in analysis/classification.tsv always win over this file.
The evidence column records the kind and, for wrappers, the callee.
"""
import argparse
import glob
import os
import re
import struct

ROOT = os.path.join(os.path.dirname(__file__), "..")
LIB_START = 0x2A7748
MAX_STMT = 8
HDR = re.compile(r"^// ---- (\S+) @ ([0-9a-f]{8})")
FLOW = re.compile(r"\b(if|while|for|do|goto|switch|case)\b|\?|LAB_")
CALL = re.compile(r"\b(FUN_[0-9a-f]{8}|[A-Za-z_]\w*)\s*\(")
NOT_CALLS = {"if", "while", "for", "switch", "return", "sizeof", "CONCAT44", "CONCAT22", "CONCAT11",
             "SUB164", "SUB168", "SUB124", "ZEXT48", "SEXT48", "ZEXT816", "SEXT816"}
DECL = re.compile(r"^\s*(undefined\d*|int|uint|short|ushort|char|byte|bool|float|long|ulong|code|"
                  r"undefined1|undefined2|undefined4|undefined8)\b[\w\s\*\[\]]*;\s*$")


def bodies(pass_):
    for f in sorted(glob.glob(os.path.join(ROOT, "analysis", "exports", pass_, "decomp", "*.c"))):
        lines = open(f, encoding="utf-8", errors="replace").read().split("\n")
        cur, buf = None, []
        for l in lines + ["// ---- END @ 00000000"]:
            m = HDR.match(l)
            if m:
                if cur:
                    yield cur, buf
                cur, buf = (m.group(1), int(m.group(2), 16)), []
            elif cur:
                buf.append(l)


def triage(name, body):
    text = "\n".join(body)
    if "{" not in text:
        return None
    inner = text[text.index("{") + 1:text.rindex("}")] if "}" in text else ""
    stmts = [s.strip() for s in inner.split("\n")
             if s.strip() and not s.strip().startswith("/*") and not DECL.match(s)]
    if any(FLOW.search(s) for s in stmts):
        return None
    if any("in_" in s or "_q" in s and "(" in s for s in stmts):  # VU0 macro / unrecovered registers
        return None
    calls = [c for s in stmts for c in CALL.findall(s) if c not in NOT_CALLS and not c.startswith("_")]
    calls = [c for c in calls if not re.match(r"^(undefined\d*|int|uint|float|code|short|char)$", c)]
    indirect = any("(**(code" in s or "(*(code" in s for s in stmts)
    real = [s for s in stmts if s not in ("return;",)]
    if not calls and not indirect:
        if not real:
            return "empty stub"
        if len(stmts) <= MAX_STMT:
            if all(s.startswith("return") for s in real):
                return "accessor/constant"
            return "setter"
        return None
    if len(calls) + indirect == 1 and len(stmts) <= 3:
        return "virtual forwarder" if indirect else f"wrapper of {calls[0]}"
    return None


def unreferenced(pass_):
    elf = open(os.path.join(ROOT, "extracted", "SCUS_974.81"), "rb").read()
    ph, n = struct.unpack_from("<I", elf, 0x1c)[0], struct.unpack_from("<H", elf, 0x2c)[0]
    segs = [struct.unpack_from("<8I", elf, ph + i * 32) for i in range(n)]
    fun = {}
    for line in open(os.path.join(ROOT, "analysis", "exports", pass_, "functions.tsv"), encoding="utf-8"):
        if line.startswith(("#", "addr	")):
            continue
        f = line.split("	")
        fun[int(f[0], 16)] = int(f[4]) if f[4].isdigit() else 1
    refs, hi = set(), {}
    for typ, off, _va, _pa, fsz, _msz, _fl, _al in segs:
        if typ != 1:
            continue
        for i in range(0, fsz - 3, 4):
            w = struct.unpack_from("<I", elf, off + i)[0]
            if w in fun:
                refs.add(w)
            op = w >> 26
            if op == 0x0F:
                hi[(w >> 16) & 31] = (w & 0xFFFF) << 16
            elif op in (0x09, 0x0D) and ((w >> 21) & 31) in hi:
                base, lo = hi[(w >> 21) & 31], w & 0xFFFF
                val = base | lo if op == 0x0D else base + (lo - 0x10000 if lo & 0x8000 else lo)
                refs.add(val)
    return {a for a, callers in fun.items() if callers == 0 and a not in refs and a < LIB_START
            and a != 0x100008}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pass", dest="pass_", default="pass7")
    ap.add_argument("--out", default=os.path.join(ROOT, "analysis", "triage.tsv"))
    ap.add_argument("--show", type=int, default=0)
    a = ap.parse_args()
    rows = []
    for (name, addr), body in bodies(a.pass_):
        if addr >= LIB_START:
            continue
        kind = triage(name, body)
        if kind:
            rows.append((addr, kind))
    dead = unreferenced(a.pass_)
    rows = [(addr, k) for addr, k in rows if addr not in dead]
    with open(a.out, "w", encoding="utf-8") as fh:
        fh.write("#addr\tstatus\tevidence (automatic, tools/triage.py)\n")
        for addr, kind in rows:
            fh.write(f"{addr:08x}\tthunk\tauto: {kind}\n")
        for addr in sorted(dead):
            fh.write(f"{addr:08x}\tdead\tauto: no caller, no data pointer, no lui/addiu reference\n")
    print(len(dead), "unreferenced (dead) game functions")
    kinds = {}
    for _a, k in rows:
        k = k.split(" of ")[0]
        kinds[k] = kinds.get(k, 0) + 1
    print(len(rows), "trivial game functions:", kinds)
    for addr, kind in rows[:a.show]:
        print(f"  {addr:08x} {kind}")


if __name__ == "__main__":
    main()
