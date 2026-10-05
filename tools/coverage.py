"""Phase-1 reverse-engineering coverage (docs/project-goal.md, "Direction").

  python tools/coverage.py [--pass pass7] [--regions] [--out analysis/coverage.tsv]

Phase 1 is done when every game-owned function is in one of five end states:
  understood  - documented well enough to reproduce its behaviour
  library     - SDK / C runtime / middleware
  dead        - unreachable or unused, with evidence
  thunk       - duplicate, thunk or trivial wrapper/accessor
  unresolved  - explicitly documented as unresolved, with reason and open questions
Anything else is work in progress:
  partial     - named, or mentioned in docs/*.md, but not yet classified as understood
  unknown     - nothing yet

Sources, strongest first:
  1. analysis/classification.tsv (addr, status, evidence): explicit end states
  2. address >= LIB_START: library (MEDIUM boundary: first 989snd.c function; no game vtable points
     past 0x2a5000 except __pure_virtual). Covers 989snd, libipu/MPEG, libgraph, libcdvd, libdbc,
     libmc2, libgcc, kernel, sifrpc, stdio, libc.
  3. analysis/triage.tsv (tools/triage.py): trivial accessors, setters, empty stubs and one-call wrappers
     found by rule in the decompilation: thunk (automatic, MEDIUM)
  4. size <= 8 bytes (two instructions: an accessor or a jump-thunk): thunk (automatic, MEDIUM)
  5. a name in analysis/symbols.tsv or the export, or the address in docs/*.md: partial (a name is not
     yet a behavioural description; promote it through classification.tsv)
"""
import argparse
import collections
import glob
import os
import re

ROOT = os.path.join(os.path.dirname(__file__), "..")
LIB_START = 0x2A7748
END = ("understood", "library", "dead", "thunk", "unresolved")
ORDER = END + ("partial", "unknown")


def load_functions(pass_):
    rows = []
    for line in open(os.path.join(ROOT, "analysis", "exports", pass_, "functions.tsv"), encoding="utf-8",
                     errors="replace"):
        if line.startswith(("#", "addr\t")):
            continue
        f = line.rstrip("\n").split("\t")
        rows.append((int(f[0], 16), f[1], int(f[2]) if f[2].isdigit() else 0))
    return rows


def load_classification(fname="classification.tsv"):
    path = os.path.join(ROOT, "analysis", fname)
    out = {}
    if os.path.exists(path):
        for line in open(path, encoding="utf-8", errors="replace"):
            if line.startswith("#") or not line.strip():
                continue
            f = line.rstrip("\n").split("\t")
            if len(f) >= 2 and f[1] in END:
                out[int(f[0], 16)] = f[1]
    return out


def load_symbols():
    names = set()
    for line in open(os.path.join(ROOT, "analysis", "symbols.tsv"), encoding="utf-8", errors="replace"):
        if line.startswith("#") or not line.strip():
            continue
        f = line.rstrip("\n").split("\t")
        if len(f) >= 4 and f[1] == "func":
            names.add(int(f[0], 16))
    return names


def doc_addresses():
    found = set()
    for p in glob.glob(os.path.join(ROOT, "docs", "*.md")):
        text = open(p, encoding="utf-8", errors="replace").read()
        found.update(int(m, 16) for m in re.findall(r"FUN_([0-9a-fA-F]{8})", text))
        found.update(int(m, 16) for m in re.findall(r"\b0x(00[12][0-9a-fA-F]{5})\b", text))
    return found


def classify(funcs):
    explicit, names, docs = load_classification(), load_symbols(), doc_addresses()
    auto = load_classification("triage.tsv")
    cls = {}
    for addr, name, size in funcs:
        if addr in explicit:
            cls[addr] = explicit[addr]
        elif addr >= LIB_START:
            cls[addr] = "library"
        elif addr in auto:
            cls[addr] = auto[addr]
        elif 0 < size <= 8:
            cls[addr] = "thunk"
        elif addr in names or not name.startswith("FUN_") or addr in docs:
            cls[addr] = "partial"
        else:
            cls[addr] = "unknown"
    return cls


def report(label, sel, cls):
    n, b = len(sel), sum(s for _a, _n, s in sel) or 1
    by, byb = collections.Counter(), collections.Counter()
    for addr, _n, size in sel:
        by[cls[addr]] += 1
        byb[cls[addr]] += size
    print(f"{label}: {n} functions, {b} bytes")
    for k in ORDER:
        print(f"  {k:10s} {by[k]:5d} ({100 * by[k] / n:5.1f} %)  {byb[k]:8d} B ({100 * byb[k] / b:5.1f} %)")
    done = sum(by[k] for k in END)
    doneb = sum(byb[k] for k in END)
    print(f"  END STATE  {done:5d} ({100 * done / n:5.1f} %)  {doneb:8d} B ({100 * doneb / b:5.1f} %)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pass", dest="pass_", default="pass7")
    ap.add_argument("--regions", action="store_true", help="also print per-64 KB region figures (game code)")
    ap.add_argument("--out", help="write a per-function TSV")
    a = ap.parse_args()
    funcs = load_functions(a.pass_)
    cls = classify(funcs)
    print(f"Ghidra export {a.pass_}")
    report("all", funcs, cls)
    report("game-owned (< LIB_START)", [f for f in funcs if f[0] < LIB_START], cls)
    if a.regions:
        reg = collections.defaultdict(collections.Counter)
        for addr, _n, _s in funcs:
            if addr < LIB_START:
                reg[addr >> 16][cls[addr]] += 1
        for r in sorted(reg):
            c = reg[r]
            t = sum(c.values())
            print(f"  0x{r << 16:08x}  {t:4d} fn  end {sum(c[k] for k in END):4d}  partial {c['partial']:4d}"
                  f"  unknown {c['unknown']:4d}")
    if a.out:
        with open(a.out, "w", encoding="utf-8") as fh:
            fh.write("addr\tname\tsize\tstatus\n")
            for addr, name, size in funcs:
                fh.write(f"{addr:08x}\t{name}\t{size}\t{cls[addr]}\n")


if __name__ == "__main__":
    main()
