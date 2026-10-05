"""Whole-map exporters: glTF 2.0, OBJ/MTL and a provenance manifest.

Policy, stated once and recorded in every manifest:

  * Geometry leaves in the game's own frame and units. No axis swap and no unit
    scale is applied unless the caller asks for one by name, and when it does the
    manifest records it as a caller assertion, not a decoded fact.
  * Texture alpha is stored in the GS nominal 0..128 range. PNG export rescales
    with min(255, round(a * 255 / 128)) and records that policy; `raw` keeps the
    stored bytes untouched.
  * Vertex colours are exported as stored bytes. Their scale is not established.
  * Normals are exported only when every normal in a primitive is non-zero, so
    no direction is ever invented. Primitives with degenerate normals ship
    without a NORMAL attribute and Blender/UE5 generate their own.
  * Alpha mode, backface culling and filtering are importer-side choices with no
    verified original counterpart, so they are flags with conservative defaults.

`require_complete` is on by default: an export that would omit any referenced
atomic or any referenced texture fails and names what is missing, rather than
writing a partial map.
"""
from array import array
import json
import math
from pathlib import Path
import struct
import zlib

from native_geometry_probe import Unsupported

GLTF_FLOAT, GLTF_UNSIGNED_BYTE = 5126, 5121
ARRAY_BUFFER = 34962

AXIS_TRANSFORMS = {
    'raw': None,
    'zup-to-yup': ((1, 0, 0), (0, 0, 1), (0, -1, 0)),
    'yup-to-zup': ((1, 0, 0), (0, 0, -1), (0, 1, 0)),
}


def write_png(path, width, height, rgba):
    def chunk(kind, body):
        return (struct.pack('>I', len(body)) + kind + body
                + struct.pack('>I', zlib.crc32(kind + body)))
    scan = b''.join(b'\0' + bytes(rgba[y * width * 4:(y + 1) * width * 4])
                    for y in range(height))
    path.write_bytes(b'\x89PNG\r\n\x1a\n'
                     + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(scan, 9)) + chunk(b'IEND', b''))


def apply_alpha_policy(rgba, policy):
    if policy == 'raw':
        return rgba
    if policy != 'gs128':
        raise Unsupported('unknown alpha policy ' + policy)
    out = bytearray(rgba)
    for i in range(3, len(out), 4):
        out[i] = min(255, round(out[i] * 255 / 128))
    return bytes(out)


class ExportPlan:
    """Material grouping, texture set and the checks that gate a full export."""

    def __init__(self, scene, resolver, require_complete=True):
        self.scene = scene
        self.resolver = resolver
        if require_complete and scene.skipped:
            reasons = sorted({s['reason'] for s in scene.skipped})
            raise Unsupported(
                f'{len(scene.skipped)} referenced atomics did not decode; '
                f'full-map export refused. Reasons: {reasons[:6]}')
        self.groups = {}
        for triangle, slot in enumerate(scene.triangle_material):
            self.groups.setdefault(slot, array('I')).append(triangle)
        self.textures = {}
        self.texture_failures = []
        for slot, material in enumerate(scene.materials):
            key = material['texture_key']
            if key is None or key in self.textures:
                continue
            hit = scene.textures.get(key)
            if hit is None:
                self.texture_failures.append({'material': slot, 'name': material['texture'],
                                              'reason': 'no dictionary hit recorded'})
                continue
            try:
                self.textures[key] = resolver.pixels(hit)
            except Unsupported as exc:
                self.texture_failures.append({'material': slot, 'name': material['texture'],
                                              'profile': hit['item'].get('profile_key'),
                                              'reason': str(exc)})
        if require_complete and self.texture_failures:
            names = sorted({f['name'] for f in self.texture_failures})
            raise Unsupported(
                f'{len(self.texture_failures)} referenced textures did not decode; '
                f'full-map export refused. Names: {names[:6]}')

    def file_names(self):
        """Stable, collision-free PNG file name per resolved texture."""
        used = {}
        names = {}
        for key in self.textures:
            base = key[1]
            safe = ''.join(c if c.isalnum() or c in '_-' else '_' for c in base) or 'texture'
            if safe in used and used[safe] != key:
                safe = f'{safe}_{key[0]:08x}'
            used[safe] = key
            names[key] = safe
        return names


def _transform(axis, unit_scale):
    if axis not in AXIS_TRANSFORMS:
        raise Unsupported('unknown axis convention ' + axis)
    matrix = AXIS_TRANSFORMS[axis]
    if matrix is None and unit_scale == 1.0:
        return None
    rows = matrix or ((1, 0, 0), (0, 1, 0), (0, 0, 1))
    return [[value * unit_scale for value in row] for row in rows]


def _apply(matrix, x, y, z):
    if matrix is None:
        return x, y, z
    return (matrix[0][0] * x + matrix[0][1] * y + matrix[0][2] * z,
            matrix[1][0] * x + matrix[1][1] * y + matrix[1][2] * z,
            matrix[2][0] * x + matrix[2][1] * y + matrix[2][2] * z)


def _normal_matrix(axis):
    """The axis transforms are signed permutations, so directions use them as-is.

    Unit scale is deliberately excluded: it must not rescale unit normals.
    """
    if axis not in AXIS_TRANSFORMS:
        raise Unsupported('unknown axis convention ' + axis)
    return AXIS_TRANSFORMS[axis]


def manifest(scene, plan, options, diagnostics):
    names = plan.file_names()
    return {
        'schema': 'WarriorsWorldExport/0.1',
        'level': scene.stem, 'worlds': scene.worlds,
        'source_identities': scene.level['source_identities'],
        'triangle_count': scene.triangle_count,
        'instances': scene.instances,
        'materials': [dict(m, image=names.get(m['texture_key'])) for m in scene.materials],
        'textures': [{
            'name': key[1], 'dictionary_offset': key[0], 'image': names[key],
            'width': image['width'], 'height': image['height'],
            'profile': image['profile'], 'levels': image['levels'],
            'source': scene.textures[key]['item']['source'],
            'sha256': scene.textures[key]['sha256']} for key, image in plan.textures.items()],
        'export_options': options,
        'coordinate_policy': (
            'Game frame preserved. Axis convention and unit scale below are caller '
            'assertions applied at write time, not decoded facts.'),
        'uv_origin_policy': (
            'Stored V is written unchanged to glTF (top-left origin) and as 1 - V '
            'to OBJ (bottom-left origin), so both render identically. Established '
            'by the verified Blender round trip, not from the executable.'),
        'diagnostics': diagnostics,
        'one_to_one_verified': False,
        'reference_render_matched': False}


def write_textures(directory, plan, alpha_policy):
    directory.mkdir(parents=True, exist_ok=True)
    names = plan.file_names()
    for key, image in plan.textures.items():
        write_png(directory / (names[key] + '.png'), image['width'], image['height'],
                  apply_alpha_policy(image['rgba_gs'], alpha_policy))
    return names


def export_gltf(scene, plan, output, options):
    """glTF 2.0 with an external .bin and PNG texture set."""
    output.mkdir(parents=True, exist_ok=True)
    names = write_textures(output / 'textures', plan, options['alpha_policy'])
    matrix = _transform(options['axis'], options['unit_scale'])
    normal_matrix = _normal_matrix(options['axis'])
    blob = bytearray()
    accessors, views, primitives = [], [], []

    def add_view(payload):
        while len(blob) % 4:
            blob.append(0)
        offset = len(blob)
        blob.extend(payload)
        views.append({'buffer': 0, 'byteOffset': offset,
                      'byteLength': len(payload), 'target': ARRAY_BUFFER})
        return len(views) - 1

    for slot in sorted(plan.groups):
        triangles = plan.groups[slot]
        count = len(triangles) * 3
        positions = array('f')
        uvs = array('f')
        normals = array('f')
        colours = bytearray()
        low = [math.inf] * 3
        high = [-math.inf] * 3
        normals_usable = True
        for triangle in triangles:
            for corner in range(triangle * 3, triangle * 3 + 3):
                x, y, z = _apply(matrix, scene.position[corner * 3],
                                 scene.position[corner * 3 + 1],
                                 scene.position[corner * 3 + 2])
                positions.extend((x, y, z))
                for k, value in enumerate((x, y, z)):
                    low[k] = min(low[k], value)
                    high[k] = max(high[k], value)
                uvs.append(scene.uv[corner * 2])
                uvs.append(scene.uv[corner * 2 + 1])
                colours.extend(scene.colour[corner * 4:corner * 4 + 4])
                nx, ny, nz = (scene.normal[corner * 4], scene.normal[corner * 4 + 1],
                              scene.normal[corner * 4 + 2])
                length = math.sqrt(nx * nx + ny * ny + nz * nz)
                if length == 0:
                    normals_usable = False
                    normals.extend((0.0, 0.0, 0.0))
                    continue
                nx, ny, nz = _apply(normal_matrix, nx / length, ny / length, nz / length)
                normals.extend((nx, ny, nz))
        attributes = {}
        accessors.append({'bufferView': add_view(positions.tobytes()),
                          'componentType': GLTF_FLOAT, 'count': count, 'type': 'VEC3',
                          'min': low, 'max': high})
        attributes['POSITION'] = len(accessors) - 1
        accessors.append({'bufferView': add_view(uvs.tobytes()),
                          'componentType': GLTF_FLOAT, 'count': count, 'type': 'VEC2'})
        attributes['TEXCOORD_0'] = len(accessors) - 1
        if options['vertex_colours']:
            accessors.append({'bufferView': add_view(bytes(colours)),
                              'componentType': GLTF_UNSIGNED_BYTE, 'normalized': True,
                              'count': count, 'type': 'VEC4'})
            attributes['COLOR_0'] = len(accessors) - 1
        if normals_usable:
            accessors.append({'bufferView': add_view(normals.tobytes()),
                              'componentType': GLTF_FLOAT, 'count': count, 'type': 'VEC3'})
            attributes['NORMAL'] = len(accessors) - 1
        primitives.append({'attributes': attributes, 'material': slot, 'mode': 4})

    samplers, images, textures, materials = [], [], [], []
    image_index = {}
    for key in plan.textures:
        image_index[key] = len(images)
        images.append({'uri': f'textures/{names[key]}.png', 'name': names[key]})
    wrap = {'WRAP': 10497, 'MIRROR': 33648, 'CLAMP': 33071, 'BORDER': 33071}
    sampler_index = {}
    for slot, material in enumerate(scene.materials):
        entry = {'name': f'{material["texture"] or "untextured"}_{slot}',
                 'doubleSided': options['double_sided'],
                 'alphaMode': options['alpha_mode'],
                 'pbrMetallicRoughness': {
                     'baseColorFactor': [c / 255 for c in material['colour']],
                     'metallicFactor': 0.0, 'roughnessFactor': 1.0}}
        key = material['texture_key']
        if key in plan.textures:
            addressing = material['addressing'] or {}
            signature = (addressing.get('address_u'), addressing.get('address_v'))
            if signature not in sampler_index:
                sampler_index[signature] = len(samplers)
                samplers.append({'magFilter': 9729, 'minFilter': 9729,
                                 'wrapS': wrap.get(signature[0], 10497),
                                 'wrapT': wrap.get(signature[1], 10497)})
            textures.append({'source': image_index[key], 'sampler': sampler_index[signature]})
            entry['pbrMetallicRoughness']['baseColorTexture'] = {'index': len(textures) - 1}
        materials.append(entry)

    (output / 'scene.bin').write_bytes(bytes(blob))
    document = {
        'asset': {'version': '2.0', 'generator': 'WarriorsWorldExport/0.1'},
        'buffers': [{'uri': 'scene.bin', 'byteLength': len(blob)}],
        'bufferViews': views, 'accessors': accessors,
        'meshes': [{'name': scene.stem, 'primitives': primitives}],
        'nodes': [{'mesh': 0, 'name': scene.stem}],
        'scenes': [{'nodes': [0]}], 'scene': 0,
        'materials': materials, 'images': images,
        'textures': textures, 'samplers': samplers}
    with (output / 'scene.gltf').open('w', encoding='utf-8') as handle:
        json.dump(document, handle, separators=(',', ':'), allow_nan=False)
    return {'primitives': len(primitives), 'buffer_bytes': len(blob),
            'images': len(images)}


def export_obj(scene, plan, output, options):
    """Wavefront OBJ + MTL. No vertex colours: the format has no standard slot."""
    output.mkdir(parents=True, exist_ok=True)
    names = write_textures(output / 'textures', plan, options['alpha_policy'])
    matrix = _transform(options['axis'], options['unit_scale'])
    with (output / 'scene.obj').open('w', encoding='utf-8') as handle:
        handle.write(f'# WarriorsWorldExport/0.1 level {scene.stem}\n')
        handle.write('mtllib scene.mtl\n')
        corner_total = len(scene.position) // 3
        for corner in range(corner_total):
            x, y, z = _apply(matrix, scene.position[corner * 3],
                             scene.position[corner * 3 + 1], scene.position[corner * 3 + 2])
            handle.write(f'v {x:.6f} {y:.6f} {z:.6f}\n')
        # OBJ places the texture origin at bottom-left, glTF at top-left. The
        # verified Blender round trip of the reference material needed 1 - V to
        # land the right way up, so OBJ flips and glTF does not. Both therefore
        # show the same image. The underlying top-left origin follows from that
        # round trip, not from the executable.
        for corner in range(corner_total):
            handle.write(f'vt {scene.uv[corner * 2]:.6f} '
                         f'{1.0 - scene.uv[corner * 2 + 1]:.6f}\n')
        for slot in sorted(plan.groups):
            material = scene.materials[slot]
            handle.write(f'g {scene.stem}_material_{slot}\n')
            handle.write(f'usemtl {material["texture"] or "untextured"}_{slot}\n')
            for triangle in plan.groups[slot]:
                a, b, c = triangle * 3 + 1, triangle * 3 + 2, triangle * 3 + 3
                handle.write(f'f {a}/{a} {b}/{b} {c}/{c}\n')
    with (output / 'scene.mtl').open('w', encoding='utf-8') as handle:
        for slot, material in enumerate(scene.materials):
            handle.write(f'newmtl {material["texture"] or "untextured"}_{slot}\n')
            red, green, blue, _ = material['colour']
            handle.write(f'Kd {red / 255:.6f} {green / 255:.6f} {blue / 255:.6f}\n')
            handle.write('Ka 0 0 0\nKs 0 0 0\nillum 1\n')
            key = material['texture_key']
            if key in plan.textures:
                handle.write(f'map_Kd textures/{names[key]}.png\n')
                handle.write(f'map_d textures/{names[key]}.png\n')
            handle.write('\n')
    return {'groups': len(plan.groups), 'vertices': len(scene.position) // 3}


def export_wmv3(scene, plan, output, options):
    """Whole-map input for the native viewer: geometry, UVs, materials, textures.

    WMV3 is a private tool format, never interpreted as a game layout. Layout:

        'WMV3', vertexCount, partCount, skipped, materialCount, textureCount
        partCount   x {first, count, member, slot, world}
        materialCount x {textureIndex | 0xffffffff, colourRGBA, flags}
        vertexCount/3 x uint16 material slot, padded to four bytes with zero
        vertexCount x float3 position
        vertexCount x float2 texture coordinate
        textureCount x {width, height, width*height*4 bytes RGBA}

    Parts stay in instance order so the viewer keeps its per-atomic isolation,
    and the material slot travels per triangle rather than per part, because a
    placed atomic draws several materials. V is written unflipped: the texture
    rows are uploaded top first, so v = 0 is the top row, matching glTF.
    """
    output.mkdir(parents=True, exist_ok=True)
    if len(scene.materials) > 65535:
        raise Unsupported(f'{len(scene.materials)} materials exceed the WMV3 table')
    keys = sorted(plan.textures)
    index = {key: position for position, key in enumerate(keys)}
    triangles = scene.triangle_count
    path = output / 'map.wmv'
    with path.open('wb') as handle:
        handle.write(struct.pack('<4sIIIII', b'WMV3', len(scene.position) // 3,
                                 len(scene.instances), len(scene.skipped),
                                 len(scene.materials), len(keys)))
        for instance in scene.instances:
            if instance['world'] > 2:
                raise Unsupported('world variant index outside the viewer range')
            handle.write(struct.pack('<5I', instance['triangle_start'] * 3,
                                     instance['triangle_count'] * 3,
                                     instance['part_member'], instance['slot'],
                                     instance['world']))
        for material in scene.materials:
            key = material['texture_key']
            red, green, blue, alpha = material['colour']
            colour = red | (green << 8) | (blue << 16) | (alpha << 24)
            handle.write(struct.pack('<3I', index.get(key, 0xffffffff), colour,
                                     material['flags'] & 0xffffffff))
        handle.write(scene.triangle_material_u16())
        if triangles % 2:
            handle.write(b'\0\0')
        handle.write(scene.position.tobytes())
        handle.write(scene.uv.tobytes())
        for key in keys:
            image = plan.textures[key]
            handle.write(struct.pack('<2I', image['width'], image['height']))
            handle.write(apply_alpha_policy(image['rgba_gs'], options['alpha_policy']))
    return {'file': 'map.wmv', 'bytes': path.stat().st_size,
            'materials': len(scene.materials), 'textures': len(keys),
            'parts': len(scene.instances)}
