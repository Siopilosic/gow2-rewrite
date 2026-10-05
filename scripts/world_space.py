"""Unified world space for a level: every decoded atomic anchored in one frame.

Anchoring rule, and the only one used:

    world_position = packed_position * atomic_plugin_0x3f0.float[0]
                                     + world_sector_plugin_0x3f1.float3

Both terms are already verified: the position scale through the VU bridge at
0x429288 and the programs at 0x501c30 / 0x5045a0, the translation through the
part-to-sector path at 0x41131c..0x411368. This module adds no rotation, no axis
swap, no unit conversion and no origin shift. A level's frame is therefore the
game's own frame for that level.

What it does NOT claim: that two different levels share an origin. Cross-level
placement has no evidence in hand, so `--all-levels` lays levels out in their own
frames and records their separate bounds instead of merging them. The axis
measurement below reports which axis is flattest; it is a measurement offered for
judgement, not a conclusion about which axis is up.

Texture resolution follows an explicit, recorded chain. The survey settled what
the first two tiers actually do: across 80,636 material references in the whole
library, 67,473 resolve in the world dictionary alone and 12,699 in the part
dictionary alone, and **not one name appears in both**. The two tiers are
disjoint per atomic, so their relative order never decides anything.

464 references (107 distinct names) resolve in neither, and 104 of those names
do exist in some other dictionary of the archive. That is what the third tier,
`level`, is for: every dictionary belonging to the level being built - each of
its world variants and each referenced part. The scope is the set of
dictionaries the level itself streams in, which is bounded and justifiable;
archive-wide search is deliberately not offered, because which dictionary a
given name resolved against at runtime is a streaming-lifetime question that has
not been traced.

Within the winning tier, multiple hits are compared byte-for-byte: identical
entries make the choice moot, differing entries are reported as a precedence
conflict rather than silently decided. Hits in later tiers are recorded as
information, not conflicts, because the chain order is the decision.
"""
from array import array
from collections import Counter, defaultdict
import hashlib
import math
import sys

from native_geometry_probe import Unsupported, take
from warriors_geometry import decode_atomic_full
import warriors_txd

CHAIN = ('part', 'world', 'level')


class TextureResolver:
    """Name -> texture record over one level's dictionary chain."""

    def __init__(self, loader, chain=CHAIN):
        self.loader = loader
        self.chain = chain
        self._dictionaries = {}
        self.conflicts = []
        self.unresolved = []

    def dictionary(self, source):
        """Read and classify one dictionary once, keyed by its WAD offset."""
        key = source['offset']
        if key not in self._dictionaries:
            self.loader.wad.seek(source['offset'])
            raw = self.loader.wad.read(source['size'])
            if len(raw) != source['size']:
                raise Unsupported('short dictionary read')
            try:
                parsed = warriors_txd.read_dictionary(raw, source['offset'], want_pixels=False)
            except Unsupported as exc:
                parsed = {'source': source, 'items': [], 'error': str(exc),
                          'declared_count': None, 'native_count': 0,
                          'count_matches': False, 'device_is_ps2': False,
                          'other_chunk_types': []}
            index = {}
            for item in parsed['items']:
                if item.get('name') is None:
                    continue
                index.setdefault(item['name'], []).append(item)
            parsed['index'] = index
            parsed['raw'] = raw
            self._dictionaries[key] = parsed
        return self._dictionaries[key]

    def resolve(self, name, sources):
        """First tier that holds the name wins; ties inside that tier are checked.

        A hit in a later tier is not a conflict - the chain order is what decides
        between tiers. A conflict is two differing entries inside the tier that
        won, where the chain gives no answer.
        """
        by_tier = {}
        order = []
        for tier, source in sources:
            if source is None or source['size'] <= 12:
                continue
            if tier not in by_tier:
                by_tier[tier] = []
                order.append(tier)
            entry = self.dictionary(source)
            for item in entry['index'].get(name, []):
                digest = hashlib.sha256(
                    take(entry['raw'], item['source']['offset'] - entry['source']['offset'],
                         item['source']['size'])).hexdigest()
                by_tier[tier].append({'tier': tier, 'dictionary': source['offset'],
                                      'item': item, 'sha256': digest})
        winning = next((tier for tier in order if by_tier[tier]), None)
        if winning is None:
            self.unresolved.append(name)
            return None, {'status': 'UNRESOLVED', 'searched': order}
        hits = by_tier[winning]
        chosen = hits[0]
        digests = {h['sha256'] for h in hits}
        later = sum(len(by_tier[tier]) for tier in order if tier != winning)
        note = {'status': 'RESOLVED', 'tier': winning,
                'dictionary': chosen['dictionary'], 'candidates': len(hits),
                'candidates_identical': len(digests) == 1,
                'candidates_in_later_tiers': later,
                'sha256': chosen['sha256']}
        if len(digests) > 1:
            note['status'] = 'PRECEDENCE_CONFLICT'
            self.conflicts.append({'name': name, 'tier': winning,
                                   'dictionaries': [h['dictionary'] for h in hits],
                                   'sha256': sorted(digests)})
        return chosen, note

    def pixels(self, hit):
        """Decode the chosen entry's level-0 image, or report why it cannot be."""
        entry = self._dictionaries[hit['dictionary']]
        item = hit['item']
        local = item['source']['offset'] - entry['source']['offset']
        chunk = take(entry['raw'], local, item['source']['size'])
        record = warriors_txd.texture_fields(chunk, item['source']['offset'])
        return warriors_txd.decode(record)


def _material_key(material, resolution):
    texture = material['texture']
    return (texture['name'] if texture else '',
            texture['mask'] if texture else '',
            texture['addressing_raw'] if texture else -1,
            tuple(material['colour']), material['flags'], material['textured'],
            resolution.get('sha256', ''))


class LevelScene:
    """Flat, world-space geometry for one level plus its diagnostics."""

    def __init__(self, stem):
        self.stem = stem
        self.position = array('f')
        self.uv = array('f')
        self.colour = array('B')
        self.normal = array('b')
        self.triangle_material = array('I')
        self.materials = []
        self._material_index = {}
        self.instances = []
        self.skipped = []
        self.scales = Counter()
        self.unresolved_worlds = []
        self.colour_extremes = [0, 0]
        self.wide_uv_atomics = 0

    def material_slot(self, material, resolution, texture_key):
        key = _material_key(material, resolution)
        if key not in self._material_index:
            texture = material['texture']
            self._material_index[key] = len(self.materials)
            self.materials.append({
                'slot': len(self.materials),
                'texture': texture['name'] if texture else None,
                'mask': texture['mask'] if texture else None,
                'texture_key': texture_key,
                'addressing': texture['addressing_interpretation'] if texture else None,
                'addressing_raw': texture['addressing_raw'] if texture else None,
                'colour': material['colour'], 'flags': material['flags'],
                'textured': material['textured'],
                'struct_hex': material['struct_hex'],
                'resolution': resolution})
        return self._material_index[key]

    @property
    def triangle_count(self):
        return len(self.triangle_material)

    def triangle_material_u16(self):
        """Per-triangle material slot as little-endian uint16, for WMV3."""
        packed = array('H', self.triangle_material)
        if len(packed) != len(self.triangle_material):
            raise Unsupported('material slot outside the 16-bit table')
        if sys.byteorder != 'little':
            packed.byteswap()
        return packed.tobytes()

    def bounds(self):
        if not self.position:
            return None
        low = [math.inf] * 3
        high = [-math.inf] * 3
        for i in range(0, len(self.position), 3):
            for k in range(3):
                value = self.position[i + k]
                low[k] = min(low[k], value)
                high[k] = max(high[k], value)
        return [low, high]


def build_level(loader, stem, chain=CHAIN, resolver=None):
    """Decode every referenced atomic of a level into one world-space buffer set."""
    scene = LevelScene(stem)
    resolver = resolver or TextureResolver(loader, chain)
    level = loader.load_level(stem)
    texture_slots = {}
    # Every dictionary this level streams in, for the `level` tier.
    level_sources = []
    seen_dictionaries = set()
    for world in level['worlds']:
        for source in ([world['layout']['texture_dictionary']]
                       + [p['texture_dictionary'] for p in world['parts']]):
            if source['size'] > 12 and source['offset'] not in seen_dictionaries:
                seen_dictionaries.add(source['offset'])
                level_sources.append(source)
    for world_index, world in enumerate(level['worlds']):
        world_dictionary = world['layout']['texture_dictionary']
        for instance in world['instances']:
            part = loader.parts[instance['part_member']]
            atomic = next(a for a in part['atomics'] if a['sector_slot'] == instance['slot'])

            def read(source):
                loader.wad.seek(source['offset'])
                return loader.wad.read(source['size'])

            try:
                decoded = decode_atomic_full(
                    read(atomic['geometry']), read(atomic['extension']),
                    atomic['geometry']['offset'], atomic['extension']['offset'],
                    detail=False)
            except Unsupported as exc:
                scene.skipped.append({'part_member': instance['part_member'],
                                      'slot': instance['slot'], 'world': world_index,
                                      'stage': 'geometry', 'reason': str(exc)})
                continue
            part_dictionary = part['texture_dictionary']
            nearby = {part_dictionary['offset'], world_dictionary['offset']}
            sources = []
            for tier in chain:
                if tier == 'part':
                    sources.append(('part', part_dictionary))
                elif tier == 'world':
                    sources.append(('world', world_dictionary))
                elif tier == 'level':
                    sources.extend(('level', source) for source in level_sources
                                   if source['offset'] not in nearby)
                else:
                    raise Unsupported('unknown dictionary tier ' + tier)
            slots = []
            failed = None
            for material in decoded['materials']:
                name = material['texture']['name'] if material['texture'] else None
                if name is None:
                    slots.append(scene.material_slot(material, {'status': 'UNTEXTURED'}, None))
                    continue
                hit, note = resolver.resolve(name, sources)
                if hit is None:
                    failed = f'unresolved texture {name}'
                    break
                key = (hit['dictionary'], name)
                texture_slots.setdefault(key, hit)
                slots.append(scene.material_slot(material, note, key))
            if failed:
                scene.skipped.append({'part_member': instance['part_member'],
                                      'slot': instance['slot'], 'world': world_index,
                                      'stage': 'texture', 'reason': failed})
                continue
            buffers = decoded['buffers']
            translation = instance['translation']
            start = scene.triangle_count
            base = len(scene.position) // 3
            for i in range(0, len(buffers['position']), 3):
                scene.position.append(buffers['position'][i] + translation[0])
                scene.position.append(buffers['position'][i + 1] + translation[1])
                scene.position.append(buffers['position'][i + 2] + translation[2])
            scene.uv.extend(buffers['uv'])
            scene.colour.extend(buffers['colour'])
            scene.normal.extend(buffers['normal'])
            for ordinal in buffers['material']:
                if ordinal >= len(slots):
                    raise Unsupported('material ordinal outside decoded list')
                scene.triangle_material.append(slots[ordinal])
            if buffers['colour']:
                scene.colour_extremes[0] = min(scene.colour_extremes[0], min(buffers['colour']))
                scene.colour_extremes[1] = max(scene.colour_extremes[1], max(buffers['colour']))
            scene.scales[decoded['position_scale']] += 1
            scene.wide_uv_atomics += bool(decoded['wide_uv_lane'])
            low = [math.inf] * 3
            high = [-math.inf] * 3
            for i in range(base * 3, len(scene.position), 3):
                for k in range(3):
                    low[k] = min(low[k], scene.position[i + k])
                    high[k] = max(high[k], scene.position[i + k])
            scene.instances.append({
                'part_member': instance['part_member'], 'slot': instance['slot'],
                'world': world_index, 'world_stem': world['world_stem'],
                'triangle_start': start, 'triangle_count': scene.triangle_count - start,
                'translation': translation,
                'translation_source': instance['translation_source'],
                'position_scale': decoded['position_scale'],
                'scale_source': decoded['scale_source'],
                'uv_scale': decoded['uv_scale'],
                'stored_sphere': decoded['stored_sphere'],
                'stored_visible_count_difference': decoded['stored_visible_count_difference'],
                'bounds': [low, high]})
    scene.worlds = [w['world_stem'] for w in level['worlds']]
    scene.unresolved_worlds = level['unresolved_worlds']
    scene.level = level
    scene.textures = texture_slots
    scene.resolver = resolver
    return scene


# --------------------------------------------------------------------------
# Diagnostics: adjacency, overlap, gaps, scale consistency, axis measurement
# --------------------------------------------------------------------------

def _relation(a, b, tolerance):
    """Per-axis gap between two AABBs. Positive gap on any axis means separated."""
    gaps = [max(a[0][k], b[0][k]) - min(a[1][k], b[1][k]) for k in range(3)]
    separation = max(gaps)
    if separation > tolerance:
        return None, 0.0
    if separation < 0.0:
        volume = 1.0
        for gap in gaps:
            volume *= -gap
        return 'overlap', volume
    return 'adjacent', 0.0


def analyse(scene, tolerance=0.0, overlap_samples=32):
    """Adjacency, overlap, coverage gaps and scale consistency in one frame."""
    instances = scene.instances
    bounds = scene.bounds()
    # Sweep on X with an active set: no cell-size choice, no quadratic blow-up on
    # the large sectors that span most of a level.
    order = sorted(range(len(instances)), key=lambda i: instances[i]['bounds'][0][0])
    active = []
    adjacency = 0
    overlaps = []
    cross_variant = 0
    for index in order:
        item = instances[index]
        low_x = item['bounds'][0][0] - tolerance
        active = [j for j in active if instances[j]['bounds'][1][0] >= low_x]
        for other in active:
            a, b = instances[other], item
            kind, volume = _relation(a['bounds'], b['bounds'], tolerance)
            if kind is None:
                continue
            if kind == 'adjacent':
                adjacency += 1
                continue
            overlaps.append({'volume': volume,
                             'a': {'part_member': a['part_member'], 'slot': a['slot'],
                                   'world': a['world_stem']},
                             'b': {'part_member': b['part_member'], 'slot': b['slot'],
                                   'world': b['world_stem']}})
            if a['world'] != b['world']:
                cross_variant += 1
        active.append(index)
    overlaps.sort(key=lambda o: -o['volume'])
    translations = [i['translation'] for i in instances]
    axis_range = []
    for k in range(3):
        if translations:
            values = [t[k] for t in translations]
            axis_range.append(max(values) - min(values))
        else:
            axis_range.append(0.0)
    widest = max(axis_range) or 1.0
    return {
        'bounds': bounds,
        'instances': len(instances),
        'triangles': scene.triangle_count,
        'adjacent_pairs': adjacency,
        'overlapping_pairs': len(overlaps),
        'overlapping_pairs_across_world_variants': cross_variant,
        'largest_overlaps': overlaps[:overlap_samples],
        'skipped': scene.skipped,
        'missing_atomics': len(scene.skipped),
        'unresolved_world_variants': getattr(scene, 'unresolved_worlds', []),
        'position_scales': {str(k): v for k, v in sorted(scene.scales.items())},
        'position_scale_consistent': len(scene.scales) == 1,
        'sector_translation_axis_range': axis_range,
        'sector_translation_axis_ratio': [value / widest for value in axis_range],
        'axis_note': 'Measurement only. A flat axis is consistent with an up axis '
                     'but is not evidence of one; no axis convention is applied.',
        'vertex_colour_component_range': scene.colour_extremes,
        'vertex_colour_note': 'If no component exceeds 128 across the corpus this is '
                              'consistent with the GS 0..128 convention; not asserted here.',
        'wide_uv_lane_atomics': scene.wide_uv_atomics,
        'texture_precedence_conflicts': scene.resolver.conflicts,
        'texture_resolution_tiers': dict(Counter(
            material['resolution'].get('tier', material['resolution']['status'])
            for material in scene.materials)),
        'unresolved_texture_names': sorted(set(scene.resolver.unresolved)),
        'one_to_one_verified': False}
