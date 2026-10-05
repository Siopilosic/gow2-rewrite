"""GOD OF WAR II .WAD container walker.

Record stream, 16-byte aligned:
  struct WadTag { u16 tag; u16 param; u32 size; char name[24]; u8 data[size]; pad to 16; }
Tag 0x00 is a named integer (value in `size`, no payload), e.g. HERO_HEAP_SIZE.
Known tags (confirmed by structure; meaning of others still TBD):
  0x00 Value   0x01 Object/Instance   0x02 GroupStart   0x03 GroupEnd
  0x13 HeaderEnd (switch from server-registration section to data section)
  0x15 WadHeader   0x16 PopHeap
Object payloads begin with a u32 server type id; in the header section the id
has bit 31 set (server instance), in the data section it is the plain id.

Usage:
  python gowwad.py tree <file.wad>
  python gowwad.py survey <dir-of-wads>
  python gowwad.py dump <file.wad> <out-dir>     # writes each tag payload to its own file
"""
import collections
import os
import struct
import sys

TAG_NAMES = {0x00: "Value", 0x01: "Object", 0x02: "GroupStart", 0x03: "GroupEnd", 0x13: "HeaderEnd",
             0x15: "WadHeader", 0x16: "PopHeap"}


def iter_tags(data, with_param=False):
    off = 0
    while off + 32 <= len(data):
        tag, param, size = struct.unpack_from("<HHI", data, off)
        name = data[off + 8:off + 32].split(b"\0")[0].decode("latin1")
        body = 0 if tag == 0x00 else size
        payload = data[off + 32: off + 32 + body]
        if with_param:
            yield off, tag, param, size, name, payload
        else:
            yield off, tag, size, name, payload
        off = (off + 32 + body + 15) & ~15


def magic_of(payload):
    return struct.unpack_from("<I", payload, 0)[0] if len(payload) >= 4 else None


def cmd_tree(path):
    data = open(path, "rb").read()
    depth = 0
    for off, tag, size, name, payload in iter_tags(data):
        if tag == 0x03:
            depth = max(0, depth - 1)
        m = magic_of(payload)
        extra = f" magic=0x{m:08x}" if tag == 0x01 and m is not None else ""
        print(f"{off:08x} {'  ' * depth}{TAG_NAMES.get(tag, f'tag_{tag:#x}')} '{name}' size={size}{extra}")
        if tag == 0x02:
            depth += 1


def cmd_survey(d):
    tags = collections.Counter()
    magics = collections.Counter()
    examples = collections.defaultdict(list)
    for fn in sorted(os.listdir(d)):
        if not fn.upper().endswith(".WAD"):
            continue
        data = open(os.path.join(d, fn), "rb").read()
        if data[:4] != b"\x15\0\0\0":
            continue
        for off, tag, size, name, payload in iter_tags(data):
            tags[tag] += 1
            if tag == 0x01:
                m = magic_of(payload)
                magics[m] += 1
                if len(examples[m]) < 4:
                    examples[m].append(name)
            elif tag not in TAG_NAMES and len(examples[("tag", tag)]) < 4:
                examples[("tag", tag)].append(f"{name}({size})")
    print("tags:")
    for t, c in sorted(tags.items()):
        print(f"  {t:#06x} {TAG_NAMES.get(t, ''):<11} x{c:<6} {examples.get(('tag', t), '')}")
    print("instance magics:")
    for m, c in sorted(magics.items(), key=lambda x: x[0] or 0):
        print(f"  0x{m:08x} x{c:<6} e.g. {examples[m]}")


def cmd_dump(path, out):
    os.makedirs(out, exist_ok=True)
    for i, (off, tag, size, name, payload) in enumerate(iter_tags(open(path, "rb").read())):
        if size:
            safe = "".join(ch if ch.isalnum() or ch in "_-." else "_" for ch in name)
            open(os.path.join(out, f"{i:05d}_{tag:02x}_{safe}.bin"), "wb").write(payload)


if __name__ == "__main__":
    {"tree": lambda: cmd_tree(sys.argv[2]), "survey": lambda: cmd_survey(sys.argv[2]),
     "dump": lambda: cmd_dump(sys.argv[2], sys.argv[3])}[sys.argv[1]]()
