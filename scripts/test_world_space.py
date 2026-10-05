"""World-space checks: dictionary precedence handling and spatial diagnostics.

Precedence has no direct executable evidence, so these checks pin the behaviour
that stands in for it: the configured chain decides between tiers, and only a
tie *inside* the winning tier - where the chain gives no answer - is reported as
a conflict. The corpus survey showed the part and world tiers never compete, so
the case that matters is two dictionaries within the `level` tier.
"""
from array import array
import io
import os
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_geometry_probe import ROOT
from world_space import LevelScene, TextureResolver, analyse

FIXTURES = Path(os.environ.get(
    'WARRIORS_FIXTURES', ROOT / 'research/reports/material-slice-verified'))
KNOWN_NAME = 'ww_opening_000'


class FakeLoader:
    """Two dictionaries laid out in one buffer, standing in for the archive."""

    def __init__(self, second=None):
        first = (FIXTURES / 'native-state/dictionary.bin').read_bytes()
        second = first if second is None else second
        self.first = {'offset': 0, 'size': len(first)}
        self.second = {'offset': len(first), 'size': len(second)}
        self.wad = io.BytesIO(first + second)


def differing_copy():
    """Same dictionary with one palette byte changed, so the digests differ."""
    raw = bytearray((FIXTURES / 'native-state/dictionary.bin').read_bytes())
    # Inside the last texture's pixel payload, so the chunk digest changes while
    # the dictionary structure stays valid.
    raw[20000] ^= 0xff
    return bytes(raw)


class Precedence(unittest.TestCase):
    """Chain order decides between tiers; only a tie inside a tier is a conflict.

    The survey established that the part and world dictionaries of an atomic are
    disjoint - across 80,636 references in the library, no name appears in both -
    so between those two tiers there is nothing to decide. What can still tie is
    two dictionaries inside the same tier, which is what the `level` tier holds.
    """

    def test_first_tier_wins(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        hit, note = resolver.resolve(KNOWN_NAME,
                                     [('part', loader.first), ('world', loader.second)])
        self.assertIsNotNone(hit)
        self.assertEqual(note['status'], 'RESOLVED')
        self.assertEqual(note['tier'], 'part')
        self.assertEqual(note['candidates'], 1)
        self.assertEqual(note['candidates_in_later_tiers'], 1)
        self.assertEqual(resolver.conflicts, [])

    def test_chain_order_selects_the_tier(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        _, note = resolver.resolve(KNOWN_NAME,
                                   [('world', loader.second), ('part', loader.first)])
        self.assertEqual(note['tier'], 'world')

    def test_a_differing_copy_in_a_later_tier_is_not_a_conflict(self):
        loader = FakeLoader(differing_copy())
        resolver = TextureResolver(loader)
        _, note = resolver.resolve(KNOWN_NAME,
                                   [('part', loader.first), ('level', loader.second)])
        self.assertEqual(note['status'], 'RESOLVED')
        self.assertEqual(note['tier'], 'part')
        self.assertEqual(note['candidates_in_later_tiers'], 1)
        self.assertEqual(resolver.conflicts, [])

    def test_differing_copies_inside_one_tier_are_a_conflict(self):
        loader = FakeLoader(differing_copy())
        resolver = TextureResolver(loader)
        _, note = resolver.resolve(KNOWN_NAME,
                                   [('level', loader.first), ('level', loader.second)])
        self.assertEqual(note['status'], 'PRECEDENCE_CONFLICT')
        self.assertEqual(note['candidates'], 2)
        self.assertFalse(note['candidates_identical'])
        self.assertEqual(len(resolver.conflicts), 1)
        self.assertEqual(resolver.conflicts[0]['name'], KNOWN_NAME)
        self.assertEqual(resolver.conflicts[0]['tier'], 'level')

    def test_identical_copies_inside_one_tier_are_moot(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        _, note = resolver.resolve(KNOWN_NAME,
                                   [('level', loader.first), ('level', loader.second)])
        self.assertEqual(note['status'], 'RESOLVED')
        self.assertEqual(note['candidates'], 2)
        self.assertTrue(note['candidates_identical'])
        self.assertEqual(resolver.conflicts, [])

    def test_missing_name_is_reported_not_substituted(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        hit, note = resolver.resolve('not_in_any_dictionary',
                                     [('part', loader.first)])
        self.assertIsNone(hit)
        self.assertEqual(note['status'], 'UNRESOLVED')
        self.assertEqual(resolver.unresolved, ['not_in_any_dictionary'])

    def test_empty_dictionary_tier_is_skipped(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        empty = {'offset': 0, 'size': 12}
        _, note = resolver.resolve(KNOWN_NAME, [('part', empty), ('world', loader.first)])
        self.assertEqual(note['tier'], 'world')
        self.assertEqual(note['candidates'], 1)

    def test_chosen_entry_decodes_to_the_verified_pixels(self):
        loader = FakeLoader()
        resolver = TextureResolver(loader)
        hit, _ = resolver.resolve(KNOWN_NAME, [('part', loader.first)])
        image = resolver.pixels(hit)
        expected = (FIXTURES / f'textures/{KNOWN_NAME}.gs.rgba').read_bytes()
        self.assertEqual(image['rgba_gs'], expected)
        self.assertEqual(image['levels'], 1)
        self.assertFalse(image['padded_upload'])


def scene_with(instances, scales=(1 / 2048,)):
    scene = LevelScene('level_test')
    scene.position = array('f')
    for low, high in (i['bounds'] for i in instances):
        scene.position.extend(low)
        scene.position.extend(high)
    scene.triangle_material = array('I', [0] * len(instances))
    scene.instances = instances
    scene.resolver = type('R', (), {'conflicts': [], 'unresolved': []})()
    for value in scales:
        scene.scales[value] += 1
    scene.colour_extremes = [0, 128]
    return scene


def instance(index, low, high, world=0, stem='level_tests'):
    return {'part_member': index, 'slot': index, 'world': world, 'world_stem': stem,
            'bounds': [list(low), list(high)], 'translation': list(low),
            'triangle_start': index, 'triangle_count': 1}


class SpatialDiagnostics(unittest.TestCase):
    def test_separated_boxes_are_neither_adjacent_nor_overlapping(self):
        report = analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1)),
                                     instance(1, (5, 5, 5), (6, 6, 6))]))
        self.assertEqual(report['adjacent_pairs'], 0)
        self.assertEqual(report['overlapping_pairs'], 0)

    def test_touching_boxes_count_as_adjacent(self):
        report = analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1)),
                                     instance(1, (1, 0, 0), (2, 1, 1))]))
        self.assertEqual(report['adjacent_pairs'], 1)
        self.assertEqual(report['overlapping_pairs'], 0)

    def test_interpenetrating_boxes_report_a_volume(self):
        report = analyse(scene_with([instance(0, (0, 0, 0), (2, 2, 2)),
                                     instance(1, (1, 1, 1), (3, 3, 3))]))
        self.assertEqual(report['overlapping_pairs'], 1)
        self.assertAlmostEqual(report['largest_overlaps'][0]['volume'], 1.0)

    def test_tolerance_widens_adjacency_only(self):
        instances = [instance(0, (0, 0, 0), (1, 1, 1)), instance(1, (1.5, 0, 0), (2, 1, 1))]
        self.assertEqual(analyse(scene_with(instances))['adjacent_pairs'], 0)
        self.assertEqual(analyse(scene_with(instances), tolerance=1.0)['adjacent_pairs'], 1)

    def test_cross_variant_overlap_is_counted_separately(self):
        report = analyse(scene_with([instance(0, (0, 0, 0), (2, 2, 2), world=0, stem='as'),
                                     instance(1, (1, 1, 1), (3, 3, 3), world=1, stem='ad')]))
        self.assertEqual(report['overlapping_pairs_across_world_variants'], 1)

    def test_many_overlapping_boxes_do_not_blow_up(self):
        instances = [instance(i, (i * 0.1, 0, 0), (i * 0.1 + 1, 1, 1)) for i in range(200)]
        report = analyse(scene_with(instances))
        self.assertGreater(report['overlapping_pairs'], 0)
        self.assertEqual(report['instances'], 200)

    def test_scale_consistency_is_reported(self):
        one = analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1))]))
        self.assertTrue(one['position_scale_consistent'])
        mixed = analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1))],
                                   scales=(1 / 2048, 1 / 4096)))
        self.assertFalse(mixed['position_scale_consistent'])
        self.assertEqual(len(mixed['position_scales']), 2)

    def test_axis_ratio_is_measured_not_concluded(self):
        report = analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1)),
                                     instance(1, (100, 1, 100), (101, 2, 101))]))
        ratios = report['sector_translation_axis_ratio']
        self.assertAlmostEqual(max(ratios), 1.0)
        self.assertLess(ratios[1], 0.05)
        self.assertIn('not evidence', report['axis_note'])

    def test_missing_atomics_are_carried_into_the_report(self):
        scene = scene_with([instance(0, (0, 0, 0), (1, 1, 1))])
        scene.skipped = [{'reason': 'unsupported ADC', 'part_member': 9, 'slot': 0}]
        report = analyse(scene)
        self.assertEqual(report['missing_atomics'], 1)
        self.assertEqual(report['skipped'][0]['reason'], 'unsupported ADC')

    def test_nothing_is_ever_marked_verified(self):
        self.assertFalse(analyse(scene_with([instance(0, (0, 0, 0), (1, 1, 1))]))
                         ['one_to_one_verified'])


if __name__ == '__main__':
    unittest.main()
