"""Bounded static pipeline evidence. Does not execute VIF/VU or emit geometry."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct

from load_map import SceneLoader
from inspect_world_geometry import children
from native_geometry_probe import ROOT, ELF_HASH, Unsupported, take, words


def upper_annotation(word):
    """Deliberately partial ISA annotation; preserve every undecoded word."""
    opcode, secondary = word & 63, (word >> 6) & 31
    result = {'raw': word, 'mnemonic': 'UNDECODED',
              'immediate_lower': bool(word & 0x80000000),
              'end_flag': bool(word & 0x40000000)}
    if opcode >= 60 and secondary in (4, 5):
        result.update(mnemonic=('ITOF' if secondary == 4 else 'FTOI') + str((0, 4, 12, 15)[opcode - 60]),
                      destination_vf=(word >> 16) & 31, source_vf=(word >> 11) & 31,
                      components=''.join(c for c, bit in zip('xyzw', (24, 23, 22, 21)) if word & (1 << bit)))
    elif opcode == 63 and secondary == 11:
        result['mnemonic'] = 'NOP'
    return result


def extract_upload(data, base=0):
    """One RET DMA packet: tag VIF words + inline NOP/MPG only, no execution."""
    tag, address, _, _ = words(data, 0, 4)
    if tag & 0xffff0000 != 0x60000000 or address:
        raise Unsupported('expected plain inline RET upload packet')
    end = 16 + (tag & 65535) * 16
    if end != len(data):
        raise Unsupported('upload packet size mismatch')
    cursor = 8
    uploads, instructions, destinations = [], [], set()
    while cursor < end:
        command_offset = cursor
        command = words(data, cursor, 1)[0]
        cursor += 4
        if command == 0:
            continue
        if command >> 24 != 0x4a:
            raise Unsupported('unsupported command in microprogram upload')
        count = (command >> 16) & 255 or 256
        destination = command & 65535
        if destination + count > 2048:
            raise Unsupported('upload exceeds VU1 instruction memory')
        if destinations.intersection(range(destination, destination + count)):
            raise Unsupported('overlapping microprogram uploads')
        body = take(data, cursor, count * 8)
        uploads.append({'command_source': {'offset': base + command_offset, 'size': 4},
                        'source': {'offset': base + cursor, 'size': len(body)},
                        'destination_instruction': destination, 'instruction_count': count})
        for i, (lower, upper) in enumerate(struct.iter_unpack('<II', body)):
            annotation = upper_annotation(upper)
            instructions.append({'pc': destination + i,
                                 'source': {'offset': base + cursor + i * 8, 'size': 8},
                                 'lower_raw': lower, 'upper': annotation,
                                 'lower_role': 'IMMEDIATE_BITS' if annotation['immediate_lower'] else 'UNDECODED_INSTRUCTION'})
        destinations.update(range(destination, destination + count))
        cursor += len(body)
    if not instructions:
        raise Unsupported('empty microprogram upload')
    return {'source': {'offset': base, 'size': len(data)}, 'uploads': uploads,
            'instruction_count': len(instructions), 'instructions': instructions,
            'executed': False, 'render_ready': False}


def executable_program(data, va):
    """Resolve upload inside an ELF32 little-endian, file-backed .vutext section."""
    if take(data, 0, 7) != b'\x7fELF\x01\x01\x01':
        raise Unsupported('expected ELF32 little endian')
    table = words(data, 32, 1)[0]
    entry_size, count, names_index = struct.unpack('<3H', take(data, 46, 6))
    if entry_size != 40 or not 0 < count <= 4096 or names_index >= count:
        raise Unsupported('unsupported ELF section table')
    sections = [words(data, table + i * 40, 10) for i in range(count)]
    names_header = sections[names_index]
    names = take(data, names_header[4], names_header[5])
    matches = []
    for s in sections:
        if s[0] >= len(names):
            raise Unsupported('section name outside string table')
        end = names.find(b'\0', s[0])
        if end < 0:
            raise Unsupported('unterminated ELF section name')
        if names[s[0]:end] == b'.vutext' and s[1] == 1 and s[3] <= va < s[3] + s[5]:
            matches.append(s)
    if len(matches) != 1:
        raise Unsupported('upload is not uniquely inside file-backed .vutext')
    s = matches[0]
    section = take(data, s[4], s[5])
    relative = va - s[3]
    packet_size = 16 + (words(section, relative, 1)[0] & 65535) * 16
    result = extract_upload(take(section, relative, packet_size), s[4] + relative)
    result['virtual_address'] = va
    for i in result['instructions']:
        i['virtual_address'] = s[3] + i['source']['offset'] - s[4]
    return result


def atomic_pipeline(data, base):
    kind, size, stamp = words(data, 0, 3)
    if kind != 3 or size + 12 != len(data) or stamp != 0x1c02000a:
        raise Unsupported('atomic extension envelope mismatch')
    identifiers = []
    for kind, offset, size in children(data, 12, len(data)):
        if kind == 31:
            if size != 8:
                raise Unsupported('pipeline plugin payload size')
            vendor, pipeline = words(data, offset + 12, 2)
            identifiers.append({'vendor': vendor, 'pipeline_id': pipeline,
                                'source': {'offset': base + offset + 12, 'size': 8}})
    if len(identifiers) > 1:
        raise Unsupported('ambiguous duplicate pipeline plugins')
    return identifiers


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise Unsupported('output directory already exists')
    executable = (ROOT / 'SLUS_212.15').read_bytes()
    if hashlib.sha256(executable).hexdigest() != ELF_HASH:
        raise Unsupported('unsupported executable identity')
    # Verified initializer 0x428f30 fills 32 entries with 0x501c30,
    # then replaces entry zero with 0x5045a0 at 0x429058.
    programs = [executable_program(executable, va) for va in (0x501c30, 0x5045a0)]
    loader = SceneLoader()
    items, counts = [], Counter()
    try:
        for index, part in loader.parts.items():
            for atomic in part['atomics']:
                source = atomic['extension']
                loader.wad.seek(source['offset'])
                identifiers = atomic_pipeline(loader.wad.read(source['size']), source['offset'])
                key = 'ABSENT' if not identifiers else '%x:%x' % (identifiers[0]['vendor'], identifiers[0]['pipeline_id'])
                counts[key] += 1
                items.append({'part_member': index, 'slot': atomic['sector_slot'], 'identifiers': identifiers,
                              'material_pipeline_selection': 'REQUIRES_MATERIAL_STATE' if key == '3:30083' else 'UNRESOLVED',
                              'render_ready': False})
        report = {'parser': 'WarriorsPipelineInspector/0.1', 'source_identities': loader.identities,
                  'atomic_count': len(items), 'atomic_pipeline_counts': dict(counts),
                  'material_program_candidates': [p['virtual_address'] for p in programs],
                  'candidate_scope': '0x30084 initializer defaults; not a per-mesh runtime selection',
                  'geometry_decoded': False, 'items': items}
    finally:
        loader.close()
    args.output.mkdir(parents=True)
    with (args.output / 'pipeline-coverage.json').open('x', encoding='utf-8') as f:
        json.dump(report, f, indent=2)
    for program in programs:
        program['source_identity'] = {'file': 'SLUS_212.15', 'sha256': ELF_HASH}
        program['annotation_scope'] = 'Partial upper-opcode annotation only; no control-flow, register-state or primitive reconstruction'
        name = 'program-%08x' % program['virtual_address']
        with (args.output / (name + '.json')).open('x', encoding='utf-8') as f:
            json.dump(program, f, indent=2)
        lines = []
        for i in program['instructions']:
            u = i['upper']
            details = '' if 'components' not in u else ' .%s vf%d, vf%d' % (u['components'], u['destination_vf'], u['source_vf'])
            lines.append('%04x ELF[%08x] %08x %08x | %s%s | %s' %
                         (i['pc'], i['source']['offset'], i['lower_raw'], u['raw'], u['mnemonic'], details, i['lower_role']))
        (args.output / (name + '.txt')).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(json.dumps({'atomic_count': len(items), 'atomic_pipeline_counts': dict(counts),
                      'programs': [{'va': p['virtual_address'], 'instructions': p['instruction_count'],
                                    'uploads': len(p['uploads'])} for p in programs], 'geometry_decoded': False}))


if __name__ == '__main__':
    main()
