"""Measure the archive's raster profiles before claiming to decode them.

This writes no assets and decodes no unfamiliar format. It walks every texture
dictionary the map library can reach, reads the 64-byte raster structure that
0x4950d8 reads, and reports:

  * how many rasters fall inside the one registered, texel-verified profile;
  * for the rest, the exact field combination and the exact gate each one fails,
    so a new profile can be traced deliberately instead of guessed;
  * transfer counts per raster, which is how stored mip chains would show up;
  * texture names that appear in more than one dictionary, split into
    byte-identical duplicates (precedence is moot) and genuine conflicts
    (precedence must be decided from the original loader, not from us);
  * every material texture reference in the library and whether the part
    dictionary, the world dictionary, both or neither can satisfy it.

Run it, then read `summary.json`. Its numbers decide the next decoder work.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import time

from native_geometry_probe import Unsupported, take, ROOT
from load_map import SceneLoader
from build_world import library_stems
from warriors_geometry import materials
import warriors_txd

VERSION = 'WarriorsTextureSurvey/0.1'
COMPACT_FIELDS = ('width', 'height', 'depth', 'format_word', 'version', 'psm',
                  'tw', 'th', 'tcc', 'tfx', 'cpsm', 'csm', 'csa', 'cld',
                  'tbw', 'tbp0', 'cbp', 'tex1_mxl', 'tex1_mmag', 'tex1_mmin',
                  'tex1_mtba', 'expected_levels', 'pixel_size', 'palette_size',
                  'allocation_size',
                  'palette_offset', 'header_tail_raw', 'miptbp1', 'miptbp2')


def survey_dictionary(loader, source, origin):
    loader.wad.seek(source['offset'])
    raw = loader.wad.read(source['size'])
    if len(raw) != source['size']:
        raise Unsupported('short dictionary read')
    try:
        info = warriors_txd.texture_chunks(raw, source['offset'])
    except Unsupported as exc:
        return {'origin': origin, 'source': source, 'status': 'UNREADABLE',
                'reason': str(exc), 'items': []}
    items = []
    for entry in info['textures']:
        _, at, length = entry['chunk']
        chunk = take(raw, at, length + 12)
        item = {'source': entry['source'],
                'sha256': hashlib.sha256(chunk).hexdigest()}
        try:
            record = warriors_txd.texture_fields(chunk, source['offset'] + at)
        except Unsupported as exc:
            item.update(status='UNREADABLE', reason=str(exc))
            items.append(item)
            continue
        profile, gates = warriors_txd.classify(record)
        item.update(
            name=record['name'], mask=record['mask'],
            profile=profile, profile_key=warriors_txd.profile_key(record),
            failed_gates=gates,
            pixel_transfers=len(record['pixel_transfers']),
            palette_transfers=len(record['palette_transfers']),
            paletted=record['paletted'],
            levels_consistent=record['levels_consistent'],
            transfer_shapes=[[t['transfer_width'], t['transfer_height'],
                              t['dsax'], t['dsay'], t['image']['size']]
                             for t in record['pixel_transfers']],
            palette_shapes=[[t['transfer_width'], t['transfer_height'],
                             t['dsax'], t['dsay'], t['image']['size']]
                            for t in record['palette_transfers']],
            fields={k: record[k] for k in COMPACT_FIELDS},
            header_hex=record['header_hex'],
            status='DECODABLE' if profile else 'UNSUPPORTED_PROFILE')
        items.append(item)
    return {'origin': origin, 'source': source, 'status': 'READ',
            'declared_count': info['declared_count'], 'device': info['device'],
            'native_count': info['native_count'],
            'count_matches': info['count_matches'],
            'other_chunk_types': info['other_chunk_types'], 'items': items}


def survey_materials(loader, stems):
    """Every material texture reference in the library, with its resolution tier."""
    names_by_dictionary = {}

    def dictionary_names(source):
        key = source['offset']
        if key not in names_by_dictionary:
            if source['size'] <= 12:
                names_by_dictionary[key] = {}
            else:
                loader.wad.seek(source['offset'])
                raw = loader.wad.read(source['size'])
                table = {}
                try:
                    info = warriors_txd.texture_chunks(raw, source['offset'])
                    for entry in info['textures']:
                        _, at, length = entry['chunk']
                        chunk = take(raw, at, length + 12)
                        record = warriors_txd.texture_fields(chunk, source['offset'] + at)
                        table.setdefault(record['name'], []).append(
                            hashlib.sha256(chunk).hexdigest())
                except Unsupported:
                    pass
                names_by_dictionary[key] = table
        return names_by_dictionary[key]

    tiers = Counter()
    unresolved = Counter()
    references = 0
    untextured = 0
    seen_atomics = set()
    addressing = Counter()
    material_failures = []
    for stem in stems:
        try:
            level = loader.load_level(stem)
        except Unsupported as exc:
            material_failures.append({'level': stem, 'reason': str(exc)})
            continue
        for world in level['worlds']:
            world_names = dictionary_names(world['layout']['texture_dictionary'])
            for instance in world['instances']:
                key = (instance['part_member'], instance['slot'])
                if key in seen_atomics:
                    continue
                seen_atomics.add(key)
                part = loader.parts[instance['part_member']]
                part_names = dictionary_names(part['texture_dictionary'])
                atomic = next(a for a in part['atomics']
                              if a['sector_slot'] == instance['slot'])
                loader.wad.seek(atomic['geometry']['offset'])
                geometry = loader.wad.read(atomic['geometry']['size'])
                try:
                    entries = materials(geometry, atomic['geometry']['offset'])
                except Unsupported as exc:
                    material_failures.append({'part_member': instance['part_member'],
                                              'slot': instance['slot'],
                                              'reason': str(exc)})
                    continue
                for material in entries:
                    if material['texture'] is None:
                        untextured += 1
                        continue
                    references += 1
                    name = material['texture']['name']
                    addressing[material['texture']['addressing_raw']] += 1
                    in_part = name in part_names
                    in_world = name in world_names
                    if in_part and in_world:
                        same = set(part_names[name]) == set(world_names[name])
                        tiers['both_identical' if same else 'both_different'] += 1
                    elif in_part:
                        tiers['part_only'] += 1
                    elif in_world:
                        tiers['world_only'] += 1
                    else:
                        tiers['neither'] += 1
                        unresolved[name] += 1
    return {'texture_references': references, 'untextured_materials': untextured,
            'atomics_examined': len(seen_atomics),
            'resolution_tiers': dict(tiers),
            'unresolved_names': unresolved.most_common(200),
            'unresolved_name_count': len(unresolved),
            'addressing_words': {hex(k): v for k, v in addressing.most_common()},
            'failures': material_failures[:200],
            'failure_count': len(material_failures)}


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--output', type=Path,
                        default=ROOT / 'research/reports/texture-survey')
    parser.add_argument('--skip-materials', action='store_true')
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit('Output directory already exists; choose a new one.')
    started = time.time()
    loader = SceneLoader()
    dictionaries = []
    try:
        stems = library_stems()
        seen = set()
        for index, part in sorted(loader.parts.items()):
            source = part['texture_dictionary']
            if source['offset'] in seen:
                continue
            seen.add(source['offset'])
            dictionaries.append(survey_dictionary(loader, source, {'kind': 'part',
                                                                   'member': index}))
        for stem in stems:
            try:
                level = loader.load_level(stem)
            except Unsupported:
                continue
            for world in level['worlds']:
                source = world['layout']['texture_dictionary']
                if source['offset'] in seen:
                    continue
                seen.add(source['offset'])
                dictionaries.append(survey_dictionary(
                    loader, source, {'kind': 'world', 'stem': world['world_stem']}))
        material_report = (None if args.skip_materials
                           else survey_materials(loader, stems))
    finally:
        loader.close()

    profiles = Counter()
    gates = Counter()
    status = Counter()
    transfer_counts = Counter()
    sizes = Counter()
    versions = Counter()
    mxl = Counter()
    palette_split = Counter()
    level_mismatch = 0
    by_name = defaultdict(set)
    total = 0
    for entry in dictionaries:
        for item in entry['items']:
            total += 1
            status[item['status']] += 1
            if item['status'] == 'UNREADABLE':
                gates['UNREADABLE:' + item['reason'][:60]] += 1
                continue
            profiles[item['profile_key']] += 1
            transfer_counts[item['pixel_transfers']] += 1
            versions[item['fields']['version']] += 1
            mxl[item['fields']['tex1_mxl']] += 1
            palette_split['paletted' if item['paletted'] else 'unpaletted'] += 1
            if not item['levels_consistent']:
                level_mismatch += 1
            sizes[(item['fields']['width'], item['fields']['height'])] += 1
            for gate in item['failed_gates']:
                gates[gate] += 1
            by_name[item['name']].add(item['sha256'])
    duplicates = {name: len(digests) for name, digests in by_name.items() if len(digests) > 1}
    summary = {
        'parser': VERSION, 'seconds': round(time.time() - started, 1),
        'source_identities': loader.identities,
        'dictionaries': len(dictionaries),
        'dictionaries_unreadable': sum(1 for d in dictionaries if d['status'] != 'READ'),
        'dictionaries_count_mismatch': sum(1 for d in dictionaries
                                           if d['status'] == 'READ' and not d['count_matches']),
        'rasters': total,
        'raster_status': dict(status),
        'decodable_fraction': (round(status['DECODABLE'] / total, 4) if total else None),
        'profile_keys': dict(profiles.most_common()),
        'failed_gates': dict(gates.most_common()),
        'pixel_transfer_counts': {str(k): v for k, v in sorted(transfer_counts.items())},
        # Reader evidence at 0x4950d8: version selects the layout (0/1 legacy
        # linear blob, 2 transfer packets), level count is (tex1 >> 2) + 1, and
        # a zero palette size is a handled case rather than a malformed raster.
        'versions': {str(k): v for k, v in sorted(versions.items())},
        'stored_mxl': {str(k): v for k, v in sorted(mxl.items())},
        'palette_presence': dict(palette_split),
        'level_count_disagrees_with_mxl': level_mismatch,
        'dimensions': {f'{w}x{h}': n for (w, h), n in sizes.most_common(64)},
        'distinct_names': len(by_name),
        'names_with_differing_bytes': len(duplicates),
        'name_conflict_examples': sorted(duplicates.items(), key=lambda kv: -kv[1])[:64],
        'materials': material_report,
        'claims': 'Measurement only. No profile is decoded that is not registered '
                  'in warriors_txd.PROFILES with an evidence note.',
        'one_to_one_verified': False}
    args.output.mkdir(parents=True)
    with (args.output / 'raster-profiles.json').open('w', encoding='utf-8') as handle:
        json.dump({'parser': VERSION, 'dictionaries': dictionaries},
                  handle, separators=(',', ':'))
    with (args.output / 'summary.json').open('w', encoding='utf-8') as handle:
        json.dump(summary, handle, indent=2)
    print(json.dumps({k: v for k, v in summary.items()
                      if k not in ('source_identities', 'name_conflict_examples')},
                     indent=2)[:8000])


if __name__ == '__main__':
    main()
