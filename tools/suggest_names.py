"""Suggest function names from string evidence in analysis/functions.tsv.

Heuristics (each suggestion records its evidence):
  * "... inside snd_BankLoad"       -> snd_BankLoad
  * "Foo(): ..." / "Foo() failed"   -> Foo
Messages like "Foo returned error" are printed by Foo's *caller*, so they are
deliberately not used. Prints TSV rows in analysis/symbols.tsv format
(confidence 'med'); review before appending.
"""
import csv
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATTERNS = [
    (re.compile(r"inside (\w+)"), "{0}"),
    (re.compile(r"(\w{4,})\(\)"), "{0}"),
]


def main():
    path = os.path.join(ROOT, "analysis", "functions.tsv")
    known = set()
    sym = os.path.join(ROOT, "analysis", "symbols.tsv")
    if os.path.exists(sym):
        known = {l.split("\t")[0] for l in open(sym) if l.strip() and not l.startswith("#")}
    for r in csv.DictReader(open(path), delimiter="\t"):
        if r["addr"] in known or not r["name"].startswith("FUN_"):
            continue
        names = {}
        for s in r["strings"].split(" | "):
            for rx, fmt in PATTERNS:
                for m in rx.finditer(s):
                    names.setdefault(fmt.format(*m.groups()), s)
        if len(names) == 1:
            (n, ev), = names.items()
            print(f"{r['addr']}\tfunc\t{n}\tmed\tstring: {ev[:100]}")
        elif names:
            print(f"# {r['addr']} ambiguous: {sorted(names)}", file=sys.stderr)


if __name__ == "__main__":
    main()
