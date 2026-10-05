"""Dump ELF32 (little-endian MIPS / PS2 EE) headers, sections, segments and symbol counts."""
import struct
import sys

PT = {0: "NULL", 1: "LOAD", 2: "DYNAMIC", 3: "INTERP", 4: "NOTE", 6: "PHDR", 0x70000000: "MIPS_REGINFO"}
SHT = {0: "NULL", 1: "PROGBITS", 2: "SYMTAB", 3: "STRTAB", 4: "RELA", 8: "NOBITS", 9: "REL", 11: "DYNSYM",
       0x70000006: "MIPS_REGINFO", 0x7000000d: "MIPS_OPTIONS", 0x70000005: "MIPS_DEBUG"}


def main(path):
    d = open(path, "rb").read()
    assert d[:4] == b"\x7fELF", "not ELF"
    (e_type, e_machine, e_version, e_entry, e_phoff, e_shoff, e_flags, e_ehsize, e_phentsize, e_phnum,
     e_shentsize, e_shnum, e_shstrndx) = struct.unpack_from("<HHIIIIIHHHHHH", d, 16)
    print(f"type={e_type} machine={e_machine} entry=0x{e_entry:08x} flags=0x{e_flags:08x}")
    print(f"phnum={e_phnum} shnum={e_shnum} shstrndx={e_shstrndx}")
    for i in range(e_phnum):
        p_type, p_off, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align = struct.unpack_from(
            "<8I", d, e_phoff + i * e_phentsize)
        print(f"  seg {i}: {PT.get(p_type, hex(p_type)):<12} off=0x{p_off:06x} vaddr=0x{p_vaddr:08x} "
              f"filesz=0x{p_filesz:06x} memsz=0x{p_memsz:06x} flags={p_flags}")
    if not e_shnum:
        return
    shdrs = [struct.unpack_from("<10I", d, e_shoff + i * e_shentsize) for i in range(e_shnum)]
    strtab = shdrs[e_shstrndx]
    name = lambda off: d[strtab[4] + off: d.index(b"\0", strtab[4] + off)].decode()
    for i, s in enumerate(shdrs):
        print(f"  sec {i:2}: {name(s[0]):<20} {SHT.get(s[1], hex(s[1])):<14} addr=0x{s[3]:08x} "
              f"off=0x{s[4]:06x} size=0x{s[5]:06x}")
        if s[1] in (2, 11):
            n = s[5] // 16
            print(f"           -> {n} symbols")


if __name__ == "__main__":
    main(sys.argv[1])
