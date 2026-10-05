"""Parse the DC_* record family (WAD tags 0x0b-0x10) of a WAD.

  python tools/dcparse.py extracted/pak/R_HERO00.WAD [DC_WAD_R_Hero]

Evidence for each table (docs/kratos-data.md, pass "tag 0x10 decoding"):
  0x0b  4 bytes. Handler 0x00120588 creates a named container in the current WAD (FUN_00120ab8).
  0x0c  the data blob. Handler 0x001205e8 copies it to the heap (or keeps it if param & 0x2000);
        container+4 = blob.
  0x0d  exports: u32 count, {u32 blob_off, u32 name_off}[count]. Handler 0x00120698 registers
        "<name>_DC" -> blob + blob_off in the current dictionary.
  0x0e  imports: u32 count, {u32 blob_off, u32 name_off}[count]. Handler 0x001207b8 resolves name
        with FUN_001204c0 and stores (target - &blob[blob_off]) at blob[blob_off]: a self-relative
        pointer.
  0x0f  handler 0x00120898 ignores it (only marks a heap payload). Layout tested here:
        u32 count, {u32 hash, u32 name_off}[count], checked against the executable's hash.
  0x10  handler 0x001208b8 ignores it. Layout tested here:
        u32 count, {u32 blob_off, u32 name_off, u32 type}[count].
Name offsets are relative to the start of the record payload.

Hash (FUN_00181428(str, seed)): h = seed; for c in str: h = h*127 + toupper(c)  (32-bit).
Names are looked up as hash("_DC", seed=hash(name)) (FUN_001204f0).

Writes analysis/dc/<WAD>/{exports,imports,hashes,objects}.tsv and types.tsv.
"""
import collections
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gow_hash(s, seed=0):
    h = seed & 0xFFFFFFFF
    for ch in s.encode("latin-1"):
        c = ch - 0x20 if 0x61 <= ch <= 0x7A else ch
        if c >= 0x80:  # lb sign-extends
            c -= 0x100
        h = (h * 127 + c) & 0xFFFFFFFF
    return h


def wad_records(data):
    off = 0
    while off + 0x20 <= len(data):
        tag, param, size = struct.unpack_from("<HHI", data, off)
        name = data[off + 8:off + 0x20].split(b"\0", 1)[0].decode("latin-1")
        body = 0 if tag == 0 else size
        yield off, tag, param, name, data[off + 0x20:off + 0x20 + body]
        off += 0x20 + ((body + 15) & ~15)


def cstr(buf, o):
    e = buf.index(b"\0", o)
    return buf[o:e].decode("latin-1")


class DC:
    def __init__(self, wad_path, container=None):
        data = open(wad_path, "rb").read()
        groups = collections.defaultdict(dict)
        for off, tag, param, name, body in wad_records(data):
            if 0x0B <= tag <= 0x10:
                groups[name][tag] = (off, body)
        if container is None:
            container = max(groups, key=lambda k: len(groups[k].get(0x0C, (0, b""))[1]))
        self.name = container
        g = groups[container]
        self.blob = g[0x0C][1]
        self.exports = self._pairs(g[0x0D][1])
        self.imports = self._pairs(g[0x0E][1])
        self.hashes = self._pairs(g[0x0F][1]) if 0x0F in g else []
        self.objects = self._triples(g[0x10][1]) if 0x10 in g else []
        self.containers = sorted(groups)

    @staticmethod
    def _pairs(p):
        n = struct.unpack_from("<I", p)[0]
        return [(a, cstr(p, b)) for a, b in (struct.unpack_from("<II", p, 4 + 8 * i) for i in range(n))]

    @staticmethod
    def _triples(p):
        n = struct.unpack_from("<I", p)[0]
        return [(a, cstr(p, b), t) for a, b, t in (struct.unpack_from("<III", p, 4 + 12 * i) for i in range(n))]


def main():
    path = sys.argv[1]
    dc = DC(path, sys.argv[2] if len(sys.argv) > 2 else None)
    wad = os.path.splitext(os.path.basename(path))[0]
    out = os.path.join(ROOT, "analysis", "dc", wad)
    os.makedirs(out, exist_ok=True)
    print(f"{wad}: containers {dc.containers}; using {dc.name}, blob {len(dc.blob)} bytes")

    ok = sum(1 for h, n in dc.hashes if gow_hash(n) == h)
    print(f"0x0f hash table: {ok}/{len(dc.hashes)} entries match hash(name)")

    # object index from the 0x10 table; sizes = distance to the next object start
    objs = sorted(dc.objects)
    starts = [o for o, _, _ in objs] + [len(dc.blob)]
    with open(os.path.join(out, "objects.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("blob_off\tsize_to_next\ttype\tname\n")
        for i, (o, n, t) in enumerate(objs):
            f.write(f"{o:08x}\t{starts[i + 1] - o}\t{t:#x}\t{n}\n")
    per_type = collections.defaultdict(list)
    for i, (o, n, t) in enumerate(objs):
        per_type[t].append((n, starts[i + 1] - o))
    with open(os.path.join(out, "types.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("type\tcount\tname_prefixes\tsizes(most common)\texample\n")
        for t in sorted(per_type):
            items = per_type[t]
            pre = collections.Counter(n.rsplit("_", 1)[0] if n[-1:].isdigit() else n.split("_", 1)[0] for n, _ in items)
            sz = collections.Counter(s for _, s in items)
            f.write(f"{t:#x}\t{len(items)}\t{', '.join(f'{k}:{v}' for k, v in pre.most_common(4))}\t"
                    f"{', '.join(f'{k}x{v}' for k, v in sz.most_common(4))}\t{items[0][0]}\n")
    for nm, rows in (("exports", dc.exports), ("imports", dc.imports)):
        with open(os.path.join(out, f"{nm}.tsv"), "w", encoding="utf-8", newline="") as f:
            f.write("blob_off\tname\n")
            for o, n in rows:
                f.write(f"{o:08x}\t{n}\n")
    with open(os.path.join(out, "hashes.tsv"), "w", encoding="utf-8", newline="") as f:
        f.write("hash\tname\thash_ok\n")
        for h, n in dc.hashes:
            f.write(f"{h:08x}\t{n}\t{int(gow_hash(n) == h)}\n")
    in_range = sum(1 for o, _, _ in objs if o < len(dc.blob))
    print(f"0x10 objects: {len(objs)} ({in_range} inside the blob), {len(per_type)} type ids")
    print(f"exports {len(dc.exports)}, imports {len(dc.imports)} -> {out}")


if __name__ == "__main__":
    main()
