"""Print Ghidra decompiler output for functions from an export pass.

  python decomp.py <addr-or-name> [...]      (GOW2_EXPORT selects the pass, default pass2)
"""
import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PASS = os.environ.get("GOW2_EXPORT", "pass2")
HDR = re.compile(r"^// ---- (\S+) @ ([0-9a-f]{8})")


def index():
    out = {}
    for f in sorted(glob.glob(os.path.join(ROOT, "analysis", "exports", PASS, "decomp", "*.c"))):
        lines = open(f, encoding="utf-8", errors="replace").read().split("\n")
        start = None
        for i, l in enumerate(lines + ["// ---- END @ 00000000"]):
            m = HDR.match(l)
            if m:
                if start is not None:
                    out[key] = "\n".join(lines[start:i]).rstrip()
                key = (m.group(1), int(m.group(2), 16))
                start = i
    return out


def main():
    idx = index()
    for q in sys.argv[1:]:
        a = int(q, 16) if re.fullmatch(r"(0x)?[0-9a-fA-F]{6,8}", q) else None
        hits = [v for (n, ad), v in idx.items() if (a is not None and ad == a) or n == q]
        print("\n\n".join(hits) if hits else f"// {q}: not found in {PASS}")
        print()


if __name__ == "__main__":
    main()
