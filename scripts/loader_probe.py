"""MIPS integer-instruction xref leads; not a PS2 EE/VU decompiler."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / '.tools'))
import capstone
import struct
import json
import hashlib

root = Path(__file__).resolve().parents[2]
out = root / 'research/reports/loader'
out.mkdir(parents=True, exist_ok=True)
data = (root / 'SLUS_212.15').read_bytes()
base = 0xff000
targets = [b'DVDWadIndexPS2.cpp', b'RockWadIndexPS2.cpp', b'InitLevel.cpp', b'ChunkSystem.cpp', b'Initialize.cpp']
md = capstone.Cs(capstone.CS_ARCH_MIPS, capstone.CS_MODE_MIPS64 | capstone.CS_MODE_LITTLE_ENDIAN)
def disasm(start, end):
    lines = []
    for off in range(start-base, end-base, 4):
        if off < 0 or off+4 > len(data):
            continue
        w = struct.unpack_from('<I', data, off)[0]
        op, rs, rt, rd = w>>26, (w>>21)&31, (w>>16)&31, (w>>11)&31
        imm = (w&65535) if (w&65535)<32768 else (w&65535)-65536
        if op in (0x1e, 0x1f):
            text = f'{"lq" if op==0x1e else "sq"} r{rt}, {imm}(r{rs}) [EE opcode]'
        elif op == 0 and w&63 in (0x18,0x19) and rd:
            text = f'EE_mult{ "u" if w&63==0x19 else ""} r{rd}, r{rs}, r{rt}'
        elif op in (0x1c, 0x12):
            text = 'UNDECODED_EE_MMI_OR_COP2'
        else:
            decoded = list(md.disasm(data[off:off+4], off+base))
            text = f'{decoded[0].mnemonic} {decoded[0].op_str}' if decoded else 'UNDECODED_PS2_OR_DATA'
        lines.append(f'{off+base:08x} [{off:08x}] {data[off:off+4].hex()} {text}')
    return '\n'.join(lines)

if len(sys.argv) == 3:
    print(disasm(int(sys.argv[1], 0), int(sys.argv[2], 0)))
else:
    records = []
    for needle in targets:
        mid = data.find(needle)
        start = data.rfind(b'\0', 0, mid)+1
        # Strings may be preceded by padding; locate printable beginning.
        while start < mid and not 32 <= data[start] < 127:
            start += 1
        address = start + base
        refs = []
        for off in range(0x1000, 0x1000+4154744, 4):
            w = struct.unpack_from('<I', data, off)[0]
            if w >> 26 != 15:
                continue
            reg = (w >> 16) & 31
            upper = (w & 65535) << 16
            for j in range(1, 9):
                nxt = struct.unpack_from('<I', data, off+j*4)[0]
                op, rs, rt, imm = nxt>>26, (nxt>>21)&31, (nxt>>16)&31, nxt&65535
                if rs == reg and op in (9, 13, 25):
                    value = (upper | imm) if op == 13 else ((upper + (imm if imm < 32768 else imm-65536)) & 0xffffffff)
                    if value == address:
                        refs.append({'lui_va': off+base, 'use_va': off+j*4+base, 'warning': 'syntactic pair; register clobber/control flow requires manual verification'})
        record = {'needle':needle.decode(), 'file_offset':start, 'virtual_address':address, 'xref_candidates':refs}
        records.append(record)
        for i, r in enumerate(refs):
            (out / (needle.decode()+f'.{i}.asm.txt')).write_text(disasm(r['lui_va']-96,r['use_va']+112))
    (out/'xref-leads.json').write_text(json.dumps({'elf_sha256':hashlib.sha256(data).hexdigest(),'tool':'Capstone '+capstone.__version__,'limitations':'generic MIPS decoding; undecoded words preserved; syntactic xrefs are leads only', 'records':records},indent=2))
    print(json.dumps(records,indent=2))
