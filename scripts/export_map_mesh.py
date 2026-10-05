"""Export strictly checked static packed map strips for the native preview."""
import argparse
import json
import math
from pathlib import Path
import struct

from load_map import SceneLoader
from inspect_world_geometry import children
from native_geometry_probe import Unsupported, take, words, native_probe


def single(chunks, kind):
    found = [c for c in chunks if c[0] == kind]
    if len(found) != 1: raise Unsupported('missing/duplicate chunk ' + hex(kind))
    return found[0]


def batches(data, base, packet):
    streams = {s['command_source']['offset']: s for s in packet['unpack_inputs']}
    pending = {}; result = []
    for tag in packet['tags']:
        if tag['id'] == 3:
            stream = streams.get(tag['source']['offset'] + 12)
            if stream is None: raise Unsupported('unrecognized vertex transfer')
            address = stream['vu_address_field']
            if address in pending or address not in range(4): raise Unsupported('duplicate/unknown input lane')
            expected = ('V4_16', 'V2_16', 'V4_8', 'V4_8')[address]
            format_ok = stream['format'] == expected or (address == 1 and stream['format'] == 'V4_16')
            if not format_ok or stream['unsigned'] != (address == 2) or not stream['tops_relative']:
                raise Unsupported('unsupported packed input profile')
            pending[address] = stream
        elif tag['id'] in (1, 6):
            if tag['vif_words'] != [0, 0]: raise Unsupported('nonempty inline tag commands')
            inline = tag.get('inline_data')
            if inline['size'] == 0: continue
            if inline['size'] != 16 or set(pending) != set(range(4)):
                raise Unsupported('unsupported draw batch shape')
            command = words(data, inline['offset'] - base, 4)
            if command[0] >> 24 != 4 or command[0] & 0xffff0000 != 0x04000000:
                raise Unsupported('missing ITOP vertex count')
            n = command[0] & 65535
            if n < 3 or any(s['count'] < n for s in pending.values()): raise Unsupported('invalid batch vertex count')
            if command[1] != (0x15000000 if not result else 0x17000000):
                raise Unsupported('unexpected microprogram start/continue')
            if command[2:] != ((0x11000000, 0x11000000) if tag['id'] == 6 else (0, 0)):
                raise Unsupported('unexpected trailing draw commands')
            result.append({'count': n, 'inputs': pending, 'draw_source': inline})
            pending = {}
        else: raise Unsupported('unsupported draw tag')
    if pending or not result: raise Unsupported('unfinished draw batch')
    return result


def decode_atomic(geometry, extension, geometry_base, extension_base):
    if words(geometry, 0, 3) != (15, len(geometry)-12, 0x1c02000a): raise Unsupported('geometry envelope')
    if words(extension, 0, 3) != (3, len(extension)-12, 0x1c02000a): raise Unsupported('extension envelope')
    ac = children(extension, 12, len(extension))
    _, po, pn = single(ac, 31)
    if pn != 8 or words(extension, po+12, 2) != (3, 0x30083): raise Unsupported('untraced atomic pipeline')
    _, so, sn = single(ac, 0x3f0)
    if sn != 12: raise Unsupported('scale plugin length')
    scale, uvscale, state = struct.unpack('<ffI', take(extension, so+12, 12))
    if state or not math.isfinite(scale) or scale <= 0 or not math.isfinite(uvscale):
        raise Unsupported('unsupported scale plugin state')
    cc = children(geometry, 12, len(geometry))
    _, go, gn = single(cc, 1)
    if gn != 40: raise Unsupported('unsupported geometry struct')
    flags, triangle_count, vertex_count, morph_count = words(geometry, go+12, 4)
    if flags & 0x01000000 == 0 or morph_count != 1:
        raise Unsupported('unsupported native/morph geometry flags')
    sphere = struct.unpack('<4f', take(geometry, go+28, 16))
    if not all(math.isfinite(v) for v in sphere) or sphere[3] <= 0: raise Unsupported('invalid stored sphere')
    # Base and effect-4 pipelines share the checked packed-position conversion.
    # Effect-2 and other paths require separate position interpretation.
    _, mo, mn = single(cc, 8)
    material_chunks = children(geometry, mo+12, mo+12+mn)
    materials = [c for c in material_chunks if c[0] == 7]
    for _, m, n in materials:
        _, e, en = single(children(geometry, m+12, m+12+n), 3)
        for t, p, length in children(geometry, e+12, e+12+en):
            if t == 0x120 and (length < 4 or words(geometry, p+12, 1)[0] not in (0, 4)):
                raise Unsupported('material effect requires another pipeline')
    _, eo, en = single(cc, 3)
    plugins = children(geometry, eo+12, eo+12+en)
    _, bo, bn = single(plugins, 0x50e)
    mode, mesh_count, index_count = words(geometry, bo+12, 3)
    if mode != 1 or bn != 12 + mesh_count*8: raise Unsupported('not the bounded native triangle-strip binmesh profile')
    descriptors = [words(geometry, bo+24+i*8, 2) for i in range(mesh_count)]
    if sum(d[0] for d in descriptors) != index_count: raise Unsupported('binmesh index sum mismatch')
    _, no, nn = single(plugins, 0x510)
    native = native_probe(take(geometry, no+12, nn), geometry_base+no+12, mesh_count, True)
    output, provenance = [], []
    strip_triangle_count = 0
    bounds = [[math.inf]*3, [-math.inf]*3]
    for mesh, (indices, material) in zip(native['meshes'], descriptors):
        if material >= len(materials): raise Unsupported('material index outside list')
        if 'packets' not in mesh: raise Unsupported('unparsed native packets')
        draws = batches(geometry, geometry_base, mesh['packets'])
        if sum(b['count'] for b in draws) - 2*(len(draws)-1) != indices:
            raise Unsupported('batch overlap/count disagrees with binmesh')
        previous = None
        for draw in draws:
            ps = draw['inputs'][0]; n = draw['count']; raw = ps['raw_vectors'][:n]
            if any(v[3] != 0 for v in raw): raise Unsupported('nonzero position W requires ADC interpretation')
            if previous is not None and previous != raw[:2]: raise Unsupported('strip boundary vertices disagree')
            previous = raw[-2:]
            positions = [[v[k]*scale for k in range(3)] for v in raw]
            for v in positions:
                if math.dist(v, sphere[:3]) > sphere[3] + math.sqrt(3)*scale*.5 + sphere[3]*1e-6:
                    raise Unsupported('decoded vertex exceeds stored bounding sphere')
                for k in range(3): bounds[0][k] = min(bounds[0][k], v[k]); bounds[1][k] = max(bounds[1][k], v[k])
            for j in range(2, n):
                strip_triangle_count += 1
                order = (j-2, j-1, j) if j%2 == 0 else (j-1, j-2, j)
                a, b, c = [raw[i] for i in order]
                ab = [b[k]-a[k] for k in range(3)]; acv = [c[k]-a[k] for k in range(3)]
                cross = [ab[1]*acv[2]-ab[2]*acv[1], ab[2]*acv[0]-ab[0]*acv[2], ab[0]*acv[1]-ab[1]*acv[0]]
                if not any(cross): continue
                output.append([positions[i] for i in order])
                provenance.append({'mesh': mesh['ordinal'], 'material': material,
                                   'vertex_offsets': [ps['source']['offset'] + i*8 for i in order],
                                   'vertex_byte_size': 8, 'draw_source': draw['draw_source']})
    # Rendering is driven by the ITOP draw count and binmesh strip indices,
    # not the pre-instancing geometry triangle total. Keep that independent
    # total as a diagnostic. Zero-area strip primitives cannot add a surface.
    if not len(output) <= triangle_count <= strip_triangle_count:
        raise Unsupported(f'triangle count outside strip bounds: {len(output)} <= {triangle_count} <= {strip_triangle_count}')
    if not output: raise Unsupported('empty geometry')
    return {'triangles': output, 'primitive_sources': provenance, 'bounds': bounds,
            'stored_triangle_count': triangle_count, 'stored_vertex_count': vertex_count,
            'strip_triangle_count': strip_triangle_count,
            'zero_area_strip_triangles': strip_triangle_count-len(output),
            'stored_visible_count_difference': triangle_count-len(output),
            'stored_sphere': sphere, 'position_scale': scale,
            'scale_source': {'offset': extension_base+so+12, 'size': 4},
            'scope': 'Static packed surface geometry; original texturing, lighting and visibility not reproduced'}


def export_level(loader, level, output):
        output = Path(output)
        if output.exists(): raise Unsupported('output directory already exists')
        vertices = []; items = []; skipped = []
        scene = loader.load_level(level)
        for world_index, world in enumerate(scene['worlds']):
            for instance in world['instances']:
                part = loader.parts[instance['part_member']]
                atomic = next(a for a in part['atomics'] if a['sector_slot'] == instance['slot'])
                def read(s): loader.wad.seek(s['offset']); return loader.wad.read(s['size'])
                try:
                    decoded = decode_atomic(read(atomic['geometry']), read(atomic['extension']), atomic['geometry']['offset'], atomic['extension']['offset'])
                except Unsupported as exc:
                    skipped.append({'member': instance['part_member'], 'slot': instance['slot'], 'reason': str(exc)}); continue
                start = len(vertices)//3
                for triangle in decoded.pop('triangles'):
                    for v in triangle: vertices.append([v[k] + instance['translation'][k] for k in range(3)])
                decoded.update(part_member=instance['part_member'], slot=instance['slot'], world=world_index,
                               triangle_start=start, triangle_count=len(vertices)//3-start,
                               translation=instance['translation'], translation_source=instance['translation_source'])
                items.append(decoded)
        if not vertices: raise Unsupported('no geometry passed the static preview checks: '+str(skipped[:3]))
        report = {'parser': 'WarriorsStaticMesh/0.2', 'level': level, 'source_identities': loader.identities,
                  'worlds': [w['world_stem'] for w in scene['worlds']], 'triangle_count': len(vertices)//3,
                  'loaded_atomics': len(items), 'skipped_atomics': skipped, 'items': items,
                  'one_to_one_verified': False, 'textures_decoded': False, 'scene_unresolved': scene.get('unresolved', [])}
        output.mkdir(parents=True)
        with (output/'mesh.json').open('x', encoding='utf-8') as f: json.dump(report, f, separators=(',', ':'))
        with (output/'map.wmv').open('xb') as f:
            f.write(struct.pack('<4sIII', b'WMV1', len(vertices), len(items), len(skipped)))
            for item in items:
                f.write(struct.pack('<5I', item['triangle_start']*3, item['triangle_count']*3, item['part_member'], item['slot'], item['world']))
            for v in vertices: f.write(struct.pack('<3f', *v))
        return report


def main():
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('level'); p.add_argument('--output', type=Path, required=True)
    args = p.parse_args(); loader = SceneLoader()
    try:
        report = export_level(loader, args.level, args.output)
        print(json.dumps({k:v for k,v in report.items() if k not in ('items','source_identities','skipped_atomics')}))
        print('Skipped reasons:', sorted(set(x['reason'] for x in report['skipped_atomics'])))
    finally: loader.close()


if __name__ == '__main__': main()
