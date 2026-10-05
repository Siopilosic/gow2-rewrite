"""Full-attribute atomic decode: positions, UVs, packed colours/normals, materials.

Two output shapes share one decode path. `decode_atomic_full(..., detail=True)`
returns per-corner dictionaries with byte provenance, for inspection and for the
single-material regression. `detail=False` returns flat `array` buffers with the
same values and no provenance, which is what whole-library export uses: the full
library is 2.6M triangles and per-corner dictionaries do not fit that budget.


This extends `export_map_mesh.decode_atomic` rather than replacing it. That
function already carries the verified gates (ITOP counts, binmesh index totals,
strip overlap, stored sphere, zero position W) and is called first, unchanged,
so no geometry reaches an exporter that the existing preview would have refused.

What is added here is the *other three* unpack lanes that the verified draw
batches already describe, plus the material list:

  lane 0  V4_16 signed    position xyz, W gated to zero by decode_atomic
  lane 1  V2_16 or V4_16  texture coordinates in xy
  lane 2  V4_8 unsigned   per-vertex colour
  lane 3  V4_8 signed     per-vertex normal

Lane assignment and formats are those enforced by `export_map_mesh.batches`.
UV semantics rest on the extracted VU programs at 0x501c30 / 0x5045a0: PC 0x1f
loads VF14.xy, PC 0x23 runs ITOF0.xy, PC 0x27 multiplies by VF6.z, whose value
is the second float of atomic plugin 0x3f0. Only xy are converted, so when the
lane-1 unpack is V4_16 the extra two components are carried through raw and
marked unresolved; they are never folded into the exported UV.

Lane 2 and lane 3 are carried as raw integers with their conversion policy
stated. Colour scaling and the original lighting equation are NOT established.
"""
from array import array
import math
import struct

from native_geometry_probe import Unsupported, take, words, span, native_probe
from inspect_world_geometry import children
from export_map_mesh import single, batches, decode_atomic
from warriors_txd import read_string

CHUNK_STRUCT, CHUNK_EXTENSION, CHUNK_TEXTURE = 1, 3, 6
CHUNK_MATERIAL_LIST, CHUNK_MATERIAL = 8, 7

# librw / RenderWare convention for the type-6 texture struct word. Recorded as
# an interpretation only: it has NOT been traced in SLUS_212.15 for this build.
FILTER_MODES = {0: 'NONE', 1: 'NEAREST', 2: 'LINEAR', 3: 'MIP_NEAREST', 4: 'MIP_LINEAR',
                5: 'LINEAR_MIP_NEAREST', 6: 'LINEAR_MIP_LINEAR'}
ADDRESS_MODES = {0: 'NONE', 1: 'WRAP', 2: 'MIRROR', 3: 'CLAMP', 4: 'BORDER'}


def texture_reference(geometry, chunk, base):
    """Type-6 texture chunk: filter/addressing word, texture name, mask name."""
    _, at, length = chunk
    entries = children(geometry, at + 12, at + 12 + length)
    header = single(entries, CHUNK_STRUCT)
    if header[2] < 4:
        raise Unsupported('short texture struct')
    raw = words(geometry, header[1] + 12, 1)[0]
    strings = [c for c in entries if c[0] == 2]
    if not strings:
        raise Unsupported('texture reference without a name')
    return {
        'name': read_string(geometry, strings[0]),
        'mask': read_string(geometry, strings[1]) if len(strings) > 1 else '',
        'addressing_raw': raw,
        'addressing_interpretation': {
            'filter': FILTER_MODES.get(raw & 0xff, f'RAW_{raw & 0xff}'),
            'address_u': ADDRESS_MODES.get((raw >> 8) & 15, f'RAW_{(raw >> 8) & 15}'),
            'address_v': ADDRESS_MODES.get((raw >> 12) & 15, f'RAW_{(raw >> 12) & 15}'),
            'use_mip_levels': bool((raw >> 16) & 1),
            'source': 'librw/RenderWare field convention; NOT traced in this build'},
        'source': span(base + at, length + 12)}


def materials(geometry, base):
    """Ordered material list for one geometry chunk.

    The material struct is read as RenderWare 3.x: flags, RGBA, unused, textured,
    then ambient/specular/diffuse floats when the struct is long enough. The raw
    bytes are always retained so nothing depends on that reading.
    """
    entries = children(geometry, 12, len(geometry))
    _, list_at, list_len = single(entries, CHUNK_MATERIAL_LIST)
    list_entries = children(geometry, list_at + 12, list_at + 12 + list_len)
    list_struct = single(list_entries, CHUNK_STRUCT)
    declared = words(geometry, list_struct[1] + 12, 1)[0]
    chunks = [c for c in list_entries if c[0] == CHUNK_MATERIAL]
    if declared != len(chunks):
        raise Unsupported('material list count disagreement')
    result = []
    for ordinal, chunk in enumerate(chunks):
        _, at, length = chunk
        parts = children(geometry, at + 12, at + 12 + length)
        struct_chunk = single(parts, CHUNK_STRUCT)
        _, sat, slen = struct_chunk
        if slen < 16:
            raise Unsupported('short material struct')
        flags, red, green, blue, alpha, unused, textured = struct.unpack(
            '<IBBBBII', take(geometry, sat + 12, 16))
        item = {'ordinal': ordinal, 'flags': flags, 'colour': [red, green, blue, alpha],
                'unused_word': unused, 'textured': textured,
                'struct_source': span(base + sat + 12, slen),
                'struct_hex': take(geometry, sat + 12, slen).hex(),
                'source': span(base + at, length + 12),
                'colour_policy': 'stored RGBA bytes, unconverted; original modulation not established'}
        if slen >= 28:
            item['ambient'], item['specular'], item['diffuse'] = struct.unpack(
                '<3f', take(geometry, sat + 28, 12))
        texture_chunks = [c for c in parts if c[0] == CHUNK_TEXTURE]
        if len(texture_chunks) > 1:
            raise Unsupported('multiple textures on one material')
        item['texture'] = (texture_reference(geometry, texture_chunks[0], base)
                           if texture_chunks else None)
        extension = [c for c in parts if c[0] == CHUNK_EXTENSION]
        item['extension_plugins'] = sorted({
            c[0] for e in extension
            for c in children(geometry, e[1] + 12, e[1] + 12 + e[2])})
        if textured and not texture_chunks:
            raise Unsupported('material declares a texture but carries none')
        result.append(item)
    return result


def _lane_map(geometry, base, extension):
    """Offset of each packed position vector -> (draw batch, index within it).

    Rebuilt exactly as the verified material slice does, from the same native
    packets the triangle decode consumed.
    """
    entries = children(geometry, 12, len(geometry))
    _, ext_at, ext_len = single(entries, CHUNK_EXTENSION)
    plugins = children(geometry, ext_at + 12, ext_at + 12 + ext_len)
    _, bin_at, _ = single(plugins, 0x50e)
    _, native_at, native_len = single(plugins, 0x510)
    _, mesh_count, _ = words(geometry, bin_at + 12, 3)
    native = native_probe(take(geometry, native_at + 12, native_len),
                          base + native_at + 12, mesh_count, True)
    lanes = {}
    for mesh in native['meshes']:
        for draw in batches(geometry, base, mesh['packets']):
            position = draw['inputs'][0]
            for i in range(draw['count']):
                lanes[position['source']['offset'] + i * 8] = (draw, i)
    return lanes


def uv_scale(extension):
    """Second float of atomic plugin 0x3f0; the VU constant multiplied into UVs."""
    entries = children(extension, 12, len(extension))
    _, at, length = single(entries, 0x3f0)
    if length < 12:
        raise Unsupported('scale plugin length')
    scale, uvs, state = struct.unpack('<ffI', take(extension, at + 12, 12))
    if state or not math.isfinite(uvs):
        raise Unsupported('unsupported scale plugin state')
    return uvs, span(at + 16, 4)


def decode_atomic_full(geometry, extension, geometry_base, extension_base, detail=True):
    """Verified triangles plus per-corner UV, colour and normal attributes."""
    decoded = decode_atomic(geometry, extension, geometry_base, extension_base)
    scale, uv_source = uv_scale(extension)
    lanes = _lane_map(geometry, geometry_base, extension)
    material_list = materials(geometry, geometry_base)
    corners = []
    buffers = {'position': array('f'), 'uv': array('f'),
               'colour': array('B'), 'normal': array('b'),
               'material': array('H'), 'lane1_extra': array('h')}
    wide_uv = False
    for triangle, provenance in zip(decoded['triangles'], decoded['primitive_sources']):
        for position, offset in zip(triangle, provenance['vertex_offsets']):
            draw, index = lanes[offset]
            raw = {lane: draw['inputs'][lane]['raw_vectors'][index] for lane in range(4)}
            uv_lane = raw[1]
            if len(uv_lane) not in (2, 4):
                raise Unsupported('unsupported texture-coordinate lane width')
            if len(uv_lane) == 4:
                wide_uv = True
            if not detail:
                buffers['position'].extend(position)
                buffers['uv'].extend((uv_lane[0] * scale, uv_lane[1] * scale))
                buffers['colour'].extend(raw[2])
                buffers['normal'].extend(raw[3])
                buffers['lane1_extra'].extend(uv_lane[2:] if len(uv_lane) == 4 else (0, 0))
                continue
            corner = {
                'position': position,
                'uv': [uv_lane[0] * scale, uv_lane[1] * scale],
                'colour_raw': list(raw[2]),
                'normal_raw': list(raw[3]),
                'material': provenance['material'],
                'sources': {str(lane): span(
                    draw['inputs'][lane]['source']['offset']
                    + index * draw['inputs'][lane]['vector_stride_bytes'],
                    draw['inputs'][lane]['vector_stride_bytes']) for lane in range(4)}}
            if len(uv_lane) == 4:
                corner['lane1_extra_raw'] = list(uv_lane[2:])
            corners.append(corner)
    for provenance in decoded['primitive_sources']:
        buffers['material'].append(provenance['material'])
    if detail and len(corners) != len(decoded['triangles']) * 3:
        raise Unsupported('corner/triangle disagreement')
    if not detail:
        decoded.pop('triangles')
        decoded.pop('primitive_sources')
        decoded['buffers'] = buffers
    decoded.update(
        corners=corners, materials=material_list, uv_scale=scale,
        uv_scale_source={'offset': extension_base + uv_source['offset'], 'size': 4},
        wide_uv_lane=wide_uv,
        attribute_policy={
            'uv': 'lane1.xy * plugin 0x3f0 float[1]; VU ITOF0.xy evidence at 0x501c30 PC 0x23',
            'lane1_extra': 'carried raw when the unpack is V4_16; semantics UNRESOLVED',
            'colour': 'lane2 V4_8 unsigned, raw; GS 0..128 alpha convention assumed only at export',
            'normal': 'lane3 V4_8 signed, raw; normalisation applied at export, not here'})
    return decoded
