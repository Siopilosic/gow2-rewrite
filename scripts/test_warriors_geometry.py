"""Full-attribute atomic decode checked against the already-verified slice.

The reference atomic is member 6868 slot 0, whose 1,692 corners were validated
against original WAD bytes in `research/reports/material-slice-verified`. These
checks assert the extended decode reproduces those corners exactly and that the
lean array path carries identical values to the detailed path.
"""
import json
import os
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_geometry_probe import Unsupported, ROOT
from warriors_geometry import decode_atomic_full, materials

FIXTURES = Path(os.environ.get(
    'WARRIORS_FIXTURES', ROOT / 'research/reports/material-slice-verified'))
GEOMETRY_BASE, EXTENSION_BASE = 416036968, 416079124


def load():
    return ((FIXTURES / 'native-state/geometry.bin').read_bytes(),
            (FIXTURES / 'native-state/atomic-extension.bin').read_bytes())


class ReferenceAtomic(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        geometry, extension = load()
        cls.detail = decode_atomic_full(geometry, extension, GEOMETRY_BASE, EXTENSION_BASE)
        cls.lean = decode_atomic_full(geometry, extension, GEOMETRY_BASE,
                                      EXTENSION_BASE, detail=False)

    def test_triangle_count_matches_the_stored_total(self):
        self.assertEqual(len(self.detail['triangles']), 564)
        self.assertEqual(self.detail['stored_triangle_count'], 564)

    def test_materials_carry_their_texture_names(self):
        names = [m['texture']['name'] for m in self.detail['materials']]
        self.assertEqual(names, ['metl140_blk_000', 'metlgirdr13_blk_000',
                                 'propcony11_blk_000', 'ww_opening_000'])
        for material in self.detail['materials']:
            self.assertEqual(material['textured'], 1)
            self.assertEqual(material['colour'], [255, 255, 255, 255])

    def test_scales_are_the_stored_plugin_values(self):
        self.assertEqual(self.detail['position_scale'], 1 / 2048)
        self.assertEqual(self.detail['uv_scale'], 1 / 16384)

    def test_lean_buffers_equal_the_detailed_corners(self):
        buffers = self.lean['buffers']
        self.assertEqual(len(buffers['position']) // 3, len(self.detail['corners']))
        for index, corner in enumerate(self.detail['corners']):
            self.assertEqual(list(buffers['position'][index * 3:index * 3 + 3]),
                             corner['position'])
            self.assertEqual(list(buffers['uv'][index * 2:index * 2 + 2]), corner['uv'])
            self.assertEqual(list(buffers['colour'][index * 4:index * 4 + 4]),
                             corner['colour_raw'])
            self.assertEqual(list(buffers['normal'][index * 4:index * 4 + 4]),
                             corner['normal_raw'])

    def test_material_ordinals_are_in_range(self):
        count = len(self.detail['materials'])
        for ordinal in self.lean['buffers']['material']:
            self.assertLess(ordinal, count)

    def test_uv_lane_width_is_recorded(self):
        self.assertFalse(self.detail['wide_uv_lane'])
        for corner in self.detail['corners']:
            self.assertNotIn('lane1_extra_raw', corner)

    def test_every_corner_cites_all_four_lanes(self):
        for corner in self.detail['corners'][:64]:
            self.assertEqual(sorted(corner['sources']), ['0', '1', '2', '3'])
            for span in corner['sources'].values():
                self.assertGreaterEqual(span['offset'], GEOMETRY_BASE)


class VerifiedSliceParity(unittest.TestCase):
    """Corner-for-corner equality with the stored verified export, when present."""

    def test_matches_scene_warriors_json(self):
        path = FIXTURES / 'scene.warriors.json'
        if not path.exists():
            self.skipTest('verified slice report not present')
        reference = json.loads(path.read_text())
        geometry, extension = load()
        decoded = decode_atomic_full(geometry, extension, GEOMETRY_BASE, EXTENSION_BASE)
        self.assertEqual(len(decoded['corners']), len(reference['corners']))
        self.assertEqual(decoded['uv_scale'], reference['uv_scale'])
        for mine, theirs in zip(decoded['corners'], reference['corners']):
            self.assertEqual(mine['position'], theirs['position'])
            self.assertEqual(mine['uv'], theirs['uv'])
            self.assertEqual(mine['material'], theirs['material'])


class FailsClosed(unittest.TestCase):
    def test_truncated_geometry_is_rejected(self):
        geometry, extension = load()
        for size in range(0, len(geometry), 1361):
            with self.assertRaises(Unsupported):
                decode_atomic_full(geometry[:size], extension,
                                   GEOMETRY_BASE, EXTENSION_BASE)

    def test_truncated_extension_is_rejected(self):
        geometry, extension = load()
        for size in range(0, len(extension)):
            with self.assertRaises(Unsupported):
                decode_atomic_full(geometry, extension[:size],
                                   GEOMETRY_BASE, EXTENSION_BASE)

    def test_material_count_disagreement_is_rejected(self):
        geometry, _ = load()
        broken = bytearray(geometry)
        entries = materials(bytes(geometry), GEOMETRY_BASE)
        self.assertEqual(len(entries), 4)
        # Corrupt the declared material count in the material-list struct.
        from inspect_world_geometry import children
        from export_map_mesh import single
        top = children(bytes(geometry), 12, len(geometry))
        _, list_at, list_len = single(top, 8)
        list_struct = single(children(bytes(geometry), list_at + 12,
                                      list_at + 12 + list_len), 1)
        at = list_struct[1] + 12
        broken[at:at + 4] = (9).to_bytes(4, 'little')
        with self.assertRaises(Unsupported):
            materials(bytes(broken), GEOMETRY_BASE)


if __name__ == '__main__':
    unittest.main()
