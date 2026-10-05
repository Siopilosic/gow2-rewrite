"""Oracle summaries for the Rust ANM decoder: segment counts, failures and a checksum per ANM_ record.

  python tools/oracle_anm.py gow2-rs/crates/gow2-formats/tests/oracle/anm.tsv

Per record: every clip, every block, every segment decoded with a fresh accumulator (as tools/anm_decode.py
main does); each (slot, frame, value bits) goes into a 64-bit FNV hash. Truncated segments count as failures.
Only derived numbers are stored, no game data.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from anm_decode import clips, decode_segment, u16  # noqa: E402
from dcparse import wad_records  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WADS = ["R_HERO00", "R_ZEUS", "RHOD10", "BOG40", "ATLAS210"]
M = 0xFFFFFFFFFFFFFFFF
MAX_CLIPS = 40  # pure Python is slow: the first 40 clips of each record; tests/anm_oracle.rs uses the same cap


def kinds_of(b):
    ntr, ng = u16(b, 0x10), u16(b, 0x12)
    tracks = [struct.unpack_from("<HBB", b, 0x18 + 4 * ng + 8 * i) for i in range(ntr)]
    kinds = []
    for ttype, _b2, nsub in tracks:
        for j in range(nsub):
            kinds.append(("rot", "trans", "scale")[j % 3] if ttype == 0 else
                         "trans" if ttype in (3, 8, 10) else ("rot", "trans", "scale")[len(kinds) % 3])
    return kinds


def summarize(b):
    h, nsegs, nfail, nclips = 1469598103934665603, 0, 0, 0
    kinds = kinds_of(b)
    for c in list(clips(b))[:MAX_CLIPS]:
        nclips += 1
        for k, kind in enumerate(kinds):
            _a, nseg, _e, _f, tab, _dt = struct.unpack_from("<4HIf", b, c + 0x60 + 16 * k)
            for s in range(nseg):
                nsegs += 1
                try:
                    slot, flags, curves, _err = decode_segment(b, c + tab + 12 * s, kind)
                except (struct.error, IndexError):
                    nfail += 1
                    continue
                for sl in sorted(curves):
                    for f in sorted(curves[sl]):
                        for x in (sl, f, struct.unpack("<Q", struct.pack("<d", curves[sl][f]))[0]):
                            h = ((h ^ (x & M)) * 1099511628211) & M
    return nclips, nsegs, nfail, h


with open(sys.argv[1], "w", encoding="utf-8", newline="") as out:
    out.write("wad\tname\tclips\tsegments\tfailed\tcheck\n")
    for w in WADS:
        p = os.path.join(ROOT, "extracted", "pak", w + ".WAD")
        if not os.path.exists(p):
            continue
        d = open(p, "rb").read()
        for _o, tag, _p, n, body in wad_records(d):
            if tag == 1 and n.startswith("ANM_") and body and len(body) > 0x40:
                try:
                    nc, ns, nf, h = summarize(body)
                except (struct.error, IndexError):
                    continue
                out.write(f"{w}\t{n}\t{nc}\t{ns}\t{nf}\t{h}\n")
                out.flush()