"""Exporter checks: glTF/OBJ structure, refusal to write a partial map, policies.

The scene here is synthetic so the exporters can be exercised without the
archive. Geometry correctness is covered by `test_warriors_geometry.py`; what
these checks defend is that whatever the decoder produced reaches the file
unaltered, that a full-map export refuses to drop anything, and that no
coordinate or colour policy is applied unless it was asked for by name.
"""
from array import array
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_geometry_probe import Unsupported
import export_scene
from world_space import LevelScene

OPTIONS = {'axis': 'raw', 'unit_scale': 1.0, 'alpha_policy': 'gs128',
           'alpha_mode': 'OPAQUE', 'double_sided': True, 'vertex_colours': True}


def checkerboard(width, height):
    out = bytearray()
    for y in range(height):
        for x in range(width):
            value = 255 if (x + y) % 2 else 0
            out += bytes((value, 64, 128, 128))
    return bytes(out)


class StubResolver:
    def __init__(self, images):
        self.images = images

    def pixels(self, hit):
        image = self.images[hit['key']]
        if image is None:
            raise Unsupported('unsupported raster profile v9/PSMCT24')
        return image


def build_scene(with_texture=True, skipped=(), broken_texture=False):
    scene = LevelScene('level_test')
    scene.worlds = ['level_tests']
    scene.level = {'source_identities': {'WARRIORS.WAD': 'x' * 64}}
    scene.skipped = list(skipped)
    key = (1000, 'road_000')
    scene.position = array('f', [0, 0, 0, 1, 0, 0, 0, 1, 0,
                                 1, 0, 0, 1, 1, 0, 0, 1, 0])
    scene.uv = array('f', [0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1])
    scene.colour = array('B', bytes([200, 100, 50, 128]) * 6)
    scene.normal = array('b', bytes([0, 0, 127, 0]) * 6)
    scene.triangle_material = array('I', [0, 1])
    scene.materials = [
        {'slot': 0, 'texture': 'road_000' if with_texture else None, 'mask': '',
         'texture_key': key if with_texture else None,
         'addressing': {'address_u': 'WRAP', 'address_v': 'CLAMP'},
         'addressing_raw': 0x3102, 'colour': [255, 255, 255, 255], 'flags': 0,
         'textured': 1, 'struct_hex': '00', 'resolution': {'status': 'RESOLVED'}},
        {'slot': 1, 'texture': None, 'mask': None, 'texture_key': None,
         'addressing': None, 'addressing_raw': None,
         'colour': [10, 20, 30, 255], 'flags': 0, 'textured': 0,
         'struct_hex': '00', 'resolution': {'status': 'UNTEXTURED'}}]
    image = None if broken_texture else {
        'width': 4, 'height': 4, 'rgba_gs': checkerboard(4, 4), 'levels': 1,
        'profile': 'PS2_INDEXED_CSM1_V2', 'indices': b'', 'texel_sources': []}
    scene.textures = {key: {'key': key, 'sha256': 'a' * 64,
                            'item': {'source': {'offset': 1024, 'size': 64},
                                     'profile_key': 'v2/PSMT8/d8/cpsm0/csm0/levels1/mxl0'}}}
    resolver = StubResolver({key: image})
    return scene, resolver


def read_png(path):
    raw = path.read_bytes()
    assert raw[:8] == b'\x89PNG\r\n\x1a\n'
    cursor, width, height, pixels = 8, None, None, b''
    while cursor < len(raw):
        length, kind = struct.unpack('>I4s', raw[cursor:cursor + 8])
        body = raw[cursor + 8:cursor + 8 + length]
        assert struct.unpack('>I', raw[cursor + 8 + length:cursor + 12 + length])[0] == \
            zlib.crc32(kind + body)
        if kind == b'IHDR':
            width, height = struct.unpack('>II', body[:8])
        if kind == b'IDAT':
            pixels += zlib.decompress(body)
        cursor += 12 + length
    return width, height, pixels


class GltfExport(unittest.TestCase):
    def export(self, **kwargs):
        scene, resolver = build_scene(**kwargs)
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        result = export_scene.export_gltf(scene, plan, directory, OPTIONS)
        document = json.loads((directory / 'scene.gltf').read_text())
        return scene, plan, directory, document, result

    def test_document_is_structurally_consistent(self):
        _, _, directory, document, _ = self.export()
        blob = (directory / 'scene.bin').read_bytes()
        self.assertEqual(document['buffers'][0]['byteLength'], len(blob))
        self.assertEqual(document['asset']['version'], '2.0')
        for view in document['bufferViews']:
            self.assertLessEqual(view['byteOffset'] + view['byteLength'], len(blob))
        sizes = {'VEC2': 2, 'VEC3': 3, 'VEC4': 4}
        widths = {export_scene.GLTF_FLOAT: 4, export_scene.GLTF_UNSIGNED_BYTE: 1}
        for accessor in document['accessors']:
            expected = (accessor['count'] * sizes[accessor['type']]
                        * widths[accessor['componentType']])
            self.assertEqual(document['bufferViews'][accessor['bufferView']]['byteLength'],
                             expected)

    def test_one_primitive_per_material_with_matching_counts(self):
        scene, plan, _, document, _ = self.export()
        primitives = document['meshes'][0]['primitives']
        self.assertEqual(len(primitives), len(plan.groups))
        for primitive in primitives:
            triangles = len(plan.groups[primitive['material']])
            accessor = document['accessors'][primitive['attributes']['POSITION']]
            self.assertEqual(accessor['count'], triangles * 3)
            self.assertEqual(primitive['mode'], 4)

    def test_positions_reach_the_buffer_unaltered(self):
        scene, plan, directory, document, _ = self.export()
        blob = (directory / 'scene.bin').read_bytes()
        primitive = document['meshes'][0]['primitives'][0]
        accessor = document['accessors'][primitive['attributes']['POSITION']]
        view = document['bufferViews'][accessor['bufferView']]
        values = struct.unpack_from(f'<{accessor["count"] * 3}f', blob, view['byteOffset'])
        triangle = plan.groups[primitive['material']][0]
        self.assertEqual(list(values[:9]),
                         list(scene.position[triangle * 9:triangle * 9 + 9]))
        self.assertEqual(accessor['min'], [min(values[i::3]) for i in range(3)])
        self.assertEqual(accessor['max'], [max(values[i::3]) for i in range(3)])

    def test_texture_image_uses_the_named_alpha_policy(self):
        _, plan, directory, document, _ = self.export()
        name = document['images'][0]['uri']
        width, height, scanlines = read_png(directory / name)
        self.assertEqual((width, height), (4, 4))
        # Each scanline carries a leading filter byte; alpha 128 becomes 255.
        row = scanlines[1:1 + width * 4]
        self.assertEqual(row[3], 255)

    def test_raw_alpha_policy_leaves_bytes_alone(self):
        scene, resolver = build_scene()
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_gltf(scene, plan, directory, dict(OPTIONS, alpha_policy='raw'))
        _, _, scanlines = read_png(directory / 'textures/road_000.png')
        self.assertEqual(scanlines[4], 128)

    def test_untextured_material_has_no_base_colour_texture(self):
        _, _, _, document, _ = self.export()
        self.assertNotIn('baseColorTexture', document['materials'][1]['pbrMetallicRoughness'])
        self.assertIn('baseColorTexture', document['materials'][0]['pbrMetallicRoughness'])

    def test_sampler_follows_the_recorded_addressing(self):
        _, _, _, document, _ = self.export()
        sampler = document['samplers'][document['textures'][0]['sampler']]
        self.assertEqual(sampler['wrapS'], 10497)   # WRAP
        self.assertEqual(sampler['wrapT'], 33071)   # CLAMP

    def test_degenerate_normals_suppress_the_normal_attribute(self):
        scene, resolver = build_scene()
        scene.normal = array('b', bytes([0, 0, 0, 0]) * 6)
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_gltf(scene, plan, directory, OPTIONS)
        document = json.loads((directory / 'scene.gltf').read_text())
        for primitive in document['meshes'][0]['primitives']:
            self.assertNotIn('NORMAL', primitive['attributes'])


class CoordinatePolicy(unittest.TestCase):
    def positions(self, options):
        scene, resolver = build_scene()
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_gltf(scene, plan, directory, options)
        document = json.loads((directory / 'scene.gltf').read_text())
        blob = (directory / 'scene.bin').read_bytes()
        primitive = document['meshes'][0]['primitives'][0]
        accessor = document['accessors'][primitive['attributes']['POSITION']]
        view = document['bufferViews'][accessor['bufferView']]
        return list(struct.unpack_from(f'<{accessor["count"] * 3}f', blob,
                                       view['byteOffset']))

    def test_raw_is_the_default_and_changes_nothing(self):
        self.assertEqual(self.positions(OPTIONS)[:9],
                         [0, 0, 0, 1, 0, 0, 0, 1, 0])

    def test_named_axis_conversion_is_applied_exactly(self):
        values = self.positions(dict(OPTIONS, axis='zup-to-yup'))
        # (x, y, z) -> (x, z, -y)
        self.assertEqual(values[:9], [0, 0, 0, 1, 0, 0, 0, 0, -1])

    def test_unit_scale_is_applied_to_positions(self):
        values = self.positions(dict(OPTIONS, unit_scale=100.0))
        self.assertEqual(values[:9], [0, 0, 0, 100, 0, 0, 0, 100, 0])

    def test_unknown_axis_name_is_refused(self):
        with self.assertRaises(Unsupported):
            self.positions(dict(OPTIONS, axis='guess'))


class FullMapOnly(unittest.TestCase):
    def test_skipped_atomics_refuse_the_export(self):
        scene, resolver = build_scene(skipped=[{'reason': 'unsupported ADC state',
                                                'part_member': 1, 'slot': 0}])
        with self.assertRaises(Unsupported) as caught:
            export_scene.ExportPlan(scene, resolver)
        self.assertIn('unsupported ADC state', str(caught.exception))

    def test_undecodable_texture_refuses_the_export(self):
        scene, resolver = build_scene(broken_texture=True)
        with self.assertRaises(Unsupported) as caught:
            export_scene.ExportPlan(scene, resolver)
        self.assertIn('road_000', str(caught.exception))

    def test_partial_export_is_possible_only_when_asked_for(self):
        scene, resolver = build_scene(broken_texture=True)
        plan = export_scene.ExportPlan(scene, resolver, require_complete=False)
        self.assertEqual(len(plan.texture_failures), 1)
        directory = Path(tempfile.mkdtemp())
        export_scene.export_gltf(scene, plan, directory, OPTIONS)
        document = json.loads((directory / 'scene.gltf').read_text())
        self.assertEqual(document['images'], [])


class ObjExport(unittest.TestCase):
    def test_faces_reference_existing_vertices(self):
        scene, resolver = build_scene()
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_obj(scene, plan, directory, OPTIONS)
        text = (directory / 'scene.obj').read_text().splitlines()
        vertices = sum(1 for line in text if line.startswith('v '))
        coords = sum(1 for line in text if line.startswith('vt '))
        self.assertEqual(vertices, len(scene.position) // 3)
        self.assertEqual(coords, len(scene.uv) // 2)
        faces = [line for line in text if line.startswith('f ')]
        self.assertEqual(len(faces), scene.triangle_count)
        for face in faces:
            for token in face.split()[1:]:
                index = int(token.split('/')[0])
                self.assertTrue(1 <= index <= vertices)

    def test_v_is_flipped_for_obj_only(self):
        scene, resolver = build_scene()
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_obj(scene, plan, directory, OPTIONS)
        coords = [line.split()[1:] for line in
                  (directory / 'scene.obj').read_text().splitlines()
                  if line.startswith('vt ')]
        for index, (u, v) in enumerate(coords):
            self.assertAlmostEqual(float(u), scene.uv[index * 2], places=5)
            self.assertAlmostEqual(float(v), 1.0 - scene.uv[index * 2 + 1], places=5)

    def test_mtl_references_written_images(self):
        scene, resolver = build_scene()
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_obj(scene, plan, directory, OPTIONS)
        mtl = (directory / 'scene.mtl').read_text()
        self.assertIn('map_Kd textures/road_000.png', mtl)
        self.assertTrue((directory / 'textures/road_000.png').exists())


def parse_wmv3(raw):
    """Independent reader for the WMV3 layout, written from the spec.

    The C++ side has its own reader in warriors/preview.hpp. Two independent
    implementations agreeing on the same bytes is the check; neither is derived
    from the other.
    """
    magic, vertices, parts, skipped, materials, textures = struct.unpack_from('<4s5I', raw, 0)
    assert magic == b'WMV3', magic
    at = 24
    part_rows = []
    for _ in range(parts):
        part_rows.append(struct.unpack_from('<5I', raw, at)); at += 20
    material_rows = []
    for _ in range(materials):
        material_rows.append(struct.unpack_from('<3I', raw, at)); at += 12
    triangles = vertices // 3
    slots = list(struct.unpack_from(f'<{triangles}H', raw, at)); at += triangles * 2
    if triangles % 2:
        assert struct.unpack_from('<H', raw, at)[0] == 0, 'padding must be zero'
        at += 2
    position = list(struct.unpack_from(f'<{vertices * 3}f', raw, at)); at += vertices * 12
    uv = list(struct.unpack_from(f'<{vertices * 2}f', raw, at)); at += vertices * 8
    images = []
    for _ in range(textures):
        width, height = struct.unpack_from('<2I', raw, at); at += 8
        images.append((width, height, raw[at:at + width * height * 4]))
        at += width * height * 4
    assert at == len(raw), f'trailing bytes: {at} != {len(raw)}'
    return {'vertices': vertices, 'parts': part_rows, 'skipped': skipped,
            'materials': material_rows, 'triangle_material': slots,
            'position': position, 'uv': uv, 'textures': images}


class Wmv3Export(unittest.TestCase):
    def build(self, **kwargs):
        scene, resolver = build_scene(**kwargs)
        scene.instances = [
            {'part_member': 6868, 'slot': 0, 'world': 0,
             'triangle_start': 0, 'triangle_count': 1},
            {'part_member': 6869, 'slot': 3, 'world': 1,
             'triangle_start': 1, 'triangle_count': 1}]
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        info = export_scene.export_wmv3(scene, plan, directory, OPTIONS)
        raw = (directory / 'map.wmv').read_bytes()
        return scene, plan, info, raw, parse_wmv3(raw)

    def test_counts_and_exact_size(self):
        scene, _, info, raw, parsed = self.build()
        self.assertEqual(info['bytes'], len(raw))
        self.assertEqual(parsed['vertices'], len(scene.position) // 3)
        self.assertEqual(len(parsed['parts']), 2)
        self.assertEqual(len(parsed['materials']), len(scene.materials))
        self.assertEqual(len(parsed['triangle_material']), scene.triangle_count)

    def test_parts_tile_the_vertex_array_in_instance_order(self):
        scene, _, _, _, parsed = self.build()
        end = 0
        for (first, count, member, slot, world), instance in zip(parsed['parts'],
                                                                 scene.instances):
            self.assertEqual(first, end)
            self.assertEqual(count, instance['triangle_count'] * 3)
            self.assertEqual(member, instance['part_member'])
            self.assertEqual(slot, instance['slot'])
            self.assertEqual(world, instance['world'])
            end = first + count
        self.assertEqual(end, parsed['vertices'])

    def test_material_table_indexes_the_texture_set(self):
        scene, plan, _, _, parsed = self.build()
        keys = sorted(plan.textures)
        for row, material in zip(parsed['materials'], scene.materials):
            key = material['texture_key']
            expected = keys.index(key) if key in plan.textures else 0xffffffff
            self.assertEqual(row[0], expected)
        red, green, blue, alpha = scene.materials[1]['colour']
        self.assertEqual(parsed['materials'][1][1],
                         red | (green << 8) | (blue << 16) | (alpha << 24))

    def test_geometry_reaches_the_file_unaltered(self):
        scene, _, _, _, parsed = self.build()
        self.assertEqual(parsed['position'], list(scene.position))
        # V is not flipped for the viewer: rows upload top first, so v=0 is the
        # top row, the same convention as glTF.
        self.assertEqual(parsed['uv'], list(scene.uv))

    def test_triangle_material_matches_the_scene(self):
        scene, _, _, _, parsed = self.build()
        self.assertEqual(parsed['triangle_material'], list(scene.triangle_material))

    def test_texture_payload_uses_the_alpha_policy(self):
        _, plan, _, _, parsed = self.build()
        width, height, pixels = parsed['textures'][0]
        self.assertEqual((width, height), (4, 4))
        self.assertEqual(len(pixels), width * height * 4)
        self.assertEqual(pixels[3], 255)  # stored 128 rescaled by gs128

    def test_odd_triangle_count_is_padded_with_zero(self):
        scene, resolver = build_scene()
        scene.triangle_material = array('I', [0, 1, 0])
        scene.position.extend([2, 0, 0, 3, 0, 0, 2, 1, 0])
        scene.uv.extend([0, 0, 1, 0, 1, 1])
        scene.colour.extend(bytes([1, 2, 3, 4]) * 3)
        scene.normal.extend(bytes([0, 0, 127, 0]) * 3)
        scene.instances = [{'part_member': 1, 'slot': 0, 'world': 0,
                            'triangle_start': 0, 'triangle_count': 3}]
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        export_scene.export_wmv3(scene, plan, directory, OPTIONS)
        parsed = parse_wmv3((directory / 'map.wmv').read_bytes())
        self.assertEqual(len(parsed['triangle_material']), 3)

    def test_world_index_outside_the_viewer_range_is_refused(self):
        scene, resolver = build_scene()
        scene.instances = [{'part_member': 1, 'slot': 0, 'world': 7,
                            'triangle_start': 0, 'triangle_count': 2}]
        directory = Path(tempfile.mkdtemp())
        plan = export_scene.ExportPlan(scene, resolver)
        with self.assertRaises(Unsupported):
            export_scene.export_wmv3(scene, plan, directory, OPTIONS)


class Manifest(unittest.TestCase):
    def test_manifest_records_policies_and_sources(self):
        scene, resolver = build_scene()
        plan = export_scene.ExportPlan(scene, resolver)
        report = export_scene.manifest(scene, plan, OPTIONS, {'instances': 0})
        self.assertFalse(report['one_to_one_verified'])
        self.assertEqual(report['export_options'], OPTIONS)
        self.assertEqual(report['textures'][0]['source'], {'offset': 1024, 'size': 64})
        self.assertEqual(report['materials'][0]['image'], 'road_000')
        self.assertIsNone(report['materials'][1]['image'])


if __name__ == '__main__':
    unittest.main()
