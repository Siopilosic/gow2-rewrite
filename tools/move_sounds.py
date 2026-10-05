"""Per-move sound timeline of a character's move data (docs/audio.md).

  python tools/move_sounds.py [R_HERO00]   (reads analysis/dc/<WAD>/{actions,hashes}.tsv)

Writes analysis/dc/<WAD>/sounds.tsv: move, action kind (Sound 0x07, SoundOnEnemy 0x08, SoundWindow 0x09),
trigger (0 = in window, 1 = on hit, 3 = on kill, 5 = on blocked; docs/combat.md §6), window start/end in
normalised move time, and the sound name (hash at action +8, resolved through the 0x0f hash table).
"""
import collections
import csv
import os
import struct
import sys

ROOT = os.path.join(os.path.dirname(__file__), "..")
KINDS = {"0x7": "Sound", "0x8": "SoundOnEnemy", "0x9": "SoundWindow"}


def main():
    wad = sys.argv[1] if len(sys.argv) > 1 else "R_HERO00"
    base = os.path.join(ROOT, "analysis", "dc", wad)
    names = {}
    for line in open(os.path.join(base, "hashes.tsv"), encoding="utf-8"):
        f = line.rstrip("\n").split("\t")
        if len(f) >= 2:
            try:
                names[int(f[0], 16)] = f[1]
            except ValueError:
                pass
    rows, per = [], collections.Counter()
    for r in csv.DictReader(open(os.path.join(base, "actions.tsv"), encoding="utf-8"), delimiter="\t"):
        if r["kind"] not in KINDS:
            continue
        h = struct.unpack_from("<I", bytes.fromhex(r["raw"]), 8)[0]
        snd = names.get(h, f"{h:08x}")
        rows.append((r["move"], KINDS[r["kind"]], r["trigger"], r["win_start"], r["win_end"], snd))
        per[snd] += 1
    with open(os.path.join(base, "sounds.tsv"), "w", encoding="utf-8", newline="") as fh:
        fh.write("move\tkind\ttrigger\twin_start\twin_end\tsound\n")
        for row in rows:
            fh.write("\t".join(row) + "\n")
    print(f"{wad}: {len(rows)} sound actions, {len(per)} distinct sounds,",
          sum(1 for n in per if not n.startswith("SND_")), "unresolved")


if __name__ == "__main__":
    main()
