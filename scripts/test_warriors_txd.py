"""Texture-dictionary regression: real reference bytes plus synthetic corruption.

The reference dictionary is the one already validated texel-by-texel in
`research/reports/material-slice-verified`. These checks assert that the
generalised reader reproduces that verified output exactly, so relaxing the
single-packet assumption cannot silently change a decoded pixel.
"""
import os
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from native_geometry_probe import Unsupported, ROOT
import warriors_txd

FIXTURES = Path(os.environ.get(
    'WARRIORS_FIXTURES', ROOT / 'research/reports/material-slice-verified'))
REFERENCE_BASE = 416036880
EXPECTED = {
    'metl140_blk_000': (32, 32, 4),
    'metlgirdr13_blk_000': (64, 64, 8),
    'propcony11_blk_000': (64, 64, 8),
    'ww_opening_000': (128, 128, 8),
}


def reference_dictionary():
    return (FIXTURES / 'native-state/dictionary.bin').read_bytes()


class ReferenceDictionary(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = reference_dictionary()
        cls.parsed = warriors_txd.read_dictionary(cls.raw, REFERENCE_BASE)

    def test_header_agrees_with_contents(self):
        self.assertTrue(self.parsed['count_matches'])
        self.assertTrue(self.parsed['device_is_ps2'])
        self.assertEqual(self.parsed['declared_count'], len(EXPECTED))

    def test_every_texture_decodes_under_a_registered_profile(self):
        for item in self.parsed['items']:
            self.assertEqual(item['status'], 'DECODABLE', item)
            self.assertEqual(item['profile'], 'PS2_INDEXED_CSM1_V2')
            self.assertEqual(item['failed_gates'], [])

    def test_dimensions_match_the_verified_slice(self):
        found = {i['name']: (i['fields']['width'], i['fields']['height'],
                             i['fields']['depth']) for i in self.parsed['items']}
        self.assertEqual(found, EXPECTED)

    def test_pixels_are_byte_identical_to_the_verified_export(self):
        for item in self.parsed['items']:
            expected = (FIXTURES / f'textures/{item["name"]}.gs.rgba').read_bytes()
            self.assertEqual(item['image']['rgba_gs'], expected, item['name'])

    def test_texel_sources_stay_inside_the_dictionary_member(self):
        low = REFERENCE_BASE
        high = REFERENCE_BASE + len(self.raw)
        for item in self.parsed['items']:
            for pixel, _, palette in item['image']['texel_sources']:
                self.assertTrue(low <= pixel < high)
                self.assertTrue(low <= palette < high)

    def test_clut_destination_is_recorded_not_gated(self):
        # PAL8 entries in this dictionary upload their CLUT to a non-zero GS
        # destination. That must be reported without blocking the decode.
        destinations = {i['name']: i['image']['clut_destination']['dsay']
                        for i in self.parsed['items']}
        self.assertTrue(any(destinations.values()), destinations)


class Truncation(unittest.TestCase):
    def test_every_truncation_fails_closed(self):
        raw = reference_dictionary()
        for size in range(0, len(raw), 997):
            with self.assertRaises(Unsupported):
                warriors_txd.read_dictionary(raw[:size], REFERENCE_BASE)

    def test_wrong_root_chunk_is_rejected(self):
        raw = bytearray(reference_dictionary())
        raw[0:4] = struct.pack('<I', 0x15)
        with self.assertRaises(Unsupported):
            warriors_txd.read_dictionary(bytes(raw), REFERENCE_BASE)

    def test_bad_stamp_is_rejected(self):
        raw = bytearray(reference_dictionary())
        raw[8:12] = struct.pack('<I', 0)
        with self.assertRaises(Unsupported):
            warriors_txd.read_dictionary(bytes(raw), REFERENCE_BASE)


class ProfileGating(unittest.TestCase):
    """An unfamiliar raster must classify, never decode."""

    def fields(self):
        raw = reference_dictionary()
        info = warriors_txd.texture_chunks(raw, REFERENCE_BASE)
        chunk = info['textures'][-1]['chunk']
        from native_geometry_probe import take
        return warriors_txd.texture_fields(
            take(raw, chunk[1], chunk[2] + 12), REFERENCE_BASE + chunk[1])

    def test_unknown_version_is_not_decoded(self):
        record = self.fields()
        record['version'] = 7
        profile, gates = warriors_txd.classify(record)
        self.assertIsNone(profile)
        self.assertIn('version', gates)
        with self.assertRaises(Unsupported):
            warriors_txd.decode(record)

    def test_unregistered_psm_has_no_profile(self):
        record = self.fields()
        record['psm'] = 0  # PSMCT32: named in the table, no decoder registered
        profile, gates = warriors_txd.classify(record)
        self.assertIsNone(profile)
        self.assertEqual(gates, ['no_registered_profile'])

    def test_format_word_must_declare_mipmaps_when_levels_are_stored(self):
        # 0x8000 is the mipmap bit and correlates exactly with MXL >= 1 across
        # the archive, so a single-level raster must not carry it.
        record = self.fields()
        record['format_word'] = warriors_txd.BASE_FORMAT_WORD[8] | warriors_txd.FORMAT_MIPMAP
        profile, gates = warriors_txd.classify(record)
        self.assertIsNone(profile)
        self.assertIn('format_word', gates)

    def test_level_count_disagreement_is_reported(self):
        # 0x495170..0x495178: level count = (tex1 >> 2) + 1.
        record = self.fields()
        self.assertEqual(record['expected_levels'], 1)
        self.assertTrue(record['levels_consistent'])
        record['levels_consistent'] = False
        _, gates = warriors_txd.classify(record)
        self.assertIn('level_count_disagrees_with_mxl', gates)

    def test_unpaletted_raster_is_its_own_family_not_a_missing_palette(self):
        # 0x495348 skips the CLUT read when palette_size is zero, so an
        # unpaletted raster is legitimate and must classify as such.
        record = self.fields()
        record['paletted'] = False
        record['palette_size'] = 0
        record['palette_transfers'] = []
        self.assertIn('/nopal/', warriors_txd.profile_key(record))
        _, gates = warriors_txd.classify(record)
        self.assertIn('unpaletted', gates)

    def test_profile_key_is_stable(self):
        self.assertEqual(warriors_txd.profile_key(self.fields()),
                         'v2/PSMT8/d8/pal/cpsm0/csm0/levels1/mxl0')


class SyntheticRaster(unittest.TestCase):
    """Mip chains and narrow textures, which the reference dictionary lacks.

    The builder writes indices through the same address mapping the decoder
    reads them back with, so what these checks establish is the surrounding
    machinery: level shapes, buffer sizing, the multi-level walk, palette
    handling, and that no level reads outside its own payload. The mapping
    itself is established elsewhere - by the 25,600 texel comparisons against
    original bytes in the reference slice, and by the collision analysis over
    every upload shape the archive actually contains.
    """

    @staticmethod
    def build(width, height, depth, levels=1):
        def chunk(kind, body):
            return struct.pack('<III', kind, len(body), warriors_txd.STAMP) + body

        def string(value):
            padded = value.encode('ascii') + b'\0'
            padded += b'\0' * (-len(padded) % 4)
            return chunk(2, padded)

        payload = b''
        expected = []
        for level in range(levels):
            w, h, tw, th, nbytes, buffer_width = warriors_txd.level_shape(
                width, height, depth, level)
            packed = bytearray(nbytes)
            plane = []
            for y in range(h):
                row = []
                for x in range(w):
                    index = (x * 7 + y * 13 + level * 3) % (16 if depth == 4 else 256)
                    address = warriors_txd.index_address(x, y, buffer_width)
                    if depth == 8:
                        packed[address] = index
                    else:
                        packed[address // 2] |= index << ((address & 1) * 4)
                    row.append(index)
                plane.append(row)
            expected.append(plane)
            registers = b''.join(struct.pack('<QQ', value, register)
                                 for value, register in ((0, 0x51),
                                                         (tw | (th << 32), 0x52),
                                                         (0, 0x53)))
            payload += struct.pack('<4I', 3, 0x10000000, 14, 0) + registers
            payload += struct.pack('<4I', nbytes // 16, 0x08000000, 0, 0) + bytes(packed)
        entries = 16 if depth == 4 else 256
        clut = bytearray()
        for i in range(entries):
            clut += bytes((i & 255, (i * 3) & 255, (i * 5) & 255, 128))
        shape = warriors_txd.PALETTE_SHAPE[depth]
        clut_payload = bytes(clut) + bytes(shape[2] - len(clut))
        registers = b''.join(struct.pack('<QQ', value, register)
                             for value, register in ((0, 0x51),
                                                     (shape[0] | (shape[1] << 32), 0x52),
                                                     (0, 0x53)))
        palette = (struct.pack('<4I', 3, 0x10000000, 14, 0) + registers
                   + struct.pack('<4I', len(clut_payload) // 16, 0x08000000, 0, 0)
                   + clut_payload)
        tex0 = (((20 if depth == 4 else 19) << 20) | ((width.bit_length() - 1) << 26)
                | ((height.bit_length() - 1) << 30))
        header = struct.pack('<IIIHHQIIQQIIII', width, height, depth,
                             warriors_txd.BASE_FORMAT_WORD[depth]
                             | (warriors_txd.FORMAT_MIPMAP if levels > 1 else 0),
                             2, tex0, 0, (levels - 1) << 2, 0, 0,
                             len(payload), len(palette), 0, 0)
        raster = chunk(1, chunk(1, header) + chunk(1, payload + palette))
        body = (chunk(1, struct.pack('<II', warriors_txd.PS2_FOURCC, 0))
                + string('synthetic') + string('') + raster + chunk(3, b''))
        return chunk(0x15, body), expected

    def decode(self, width, height, depth, levels=1):
        raw, expected = self.build(width, height, depth, levels)
        record = warriors_txd.texture_fields(raw, 0)
        profile, gates = warriors_txd.classify(record)
        self.assertEqual(gates, [], f'{width}x{height} d{depth} levels={levels}')
        self.assertEqual(profile, 'PS2_INDEXED_CSM1_V2')
        return warriors_txd.decode(record), expected

    def test_unpadded_textures_round_trip(self):
        for width, height, depth in ((64, 64, 8), (64, 64, 4), (128, 128, 8),
                                     (32, 32, 4), (16, 16, 8), (128, 32, 8)):
            image, expected = self.decode(width, height, depth)
            self.assertEqual(image['levels'], 1)
            self.assertFalse(image['padded_upload'])
            self.assertEqual(len(image['indices']), width * height)
            self.assertEqual(list(image['indices']),
                             [v for row in expected[0] for v in row])

    def test_narrow_textures_use_the_padded_buffer_width(self):
        # Every shape in the archive that needs a padded upload.
        for width, height, depth in ((16, 16, 4), (8, 8, 4), (4, 4, 4),
                                     (8, 8, 8), (8, 16, 8), (8, 32, 8)):
            image, expected = self.decode(width, height, depth)
            self.assertTrue(image['padded_upload'],
                            f'{width}x{height} d{depth} should be padded')
            stored = image['images'][0]
            self.assertEqual(stored['buffer_width'], 2 * stored['transfer_width'])
            self.assertGreater(stored['buffer_width'], width)
            self.assertEqual(list(image['indices']),
                             [v for row in expected[0] for v in row])

    def test_mip_chains_decode_every_stored_level(self):
        for width, height, depth, levels in ((64, 64, 4, 4), (64, 64, 8, 4),
                                             (128, 128, 8, 5), (32, 32, 4, 3)):
            image, expected = self.decode(width, height, depth, levels)
            self.assertEqual(image['levels'], levels)
            self.assertEqual(len(image['images']), levels)
            for level, stored in enumerate(image['images']):
                self.assertEqual(stored['width'], max(width >> level, 1))
                self.assertEqual(stored['height'], max(height >> level, 1))
                self.assertEqual(list(stored['indices']),
                                 [v for row in expected[level] for v in row])
            self.assertEqual(image['rgba_gs'], image['images'][0]['rgba_gs'])
            self.assertEqual((image['width'], image['height']), (width, height))

    def test_rgba_follows_the_palette(self):
        image, _ = self.decode(64, 64, 8)
        for position in (0, 1, 500, 4095):
            entry = warriors_txd.clut_index(image['indices'][position], 8)
            self.assertEqual(image['rgba_gs'][position * 4:position * 4 + 4],
                             bytes((entry & 255, (entry * 3) & 255,
                                    (entry * 5) & 255, 128)))

    def test_a_wrong_transfer_shape_is_refused(self):
        raw, _ = self.build(64, 64, 8)
        record = warriors_txd.texture_fields(raw, 0)
        record['pixel_transfers'][0]['transfer_width'] = 31
        _, gates = warriors_txd.classify(record)
        self.assertIn('pixel_transfer_shape_level0', gates)

    def test_every_truncation_of_a_mipped_raster_fails_closed(self):
        raw, _ = self.build(64, 64, 4, 4)
        for size in range(0, len(raw), 251):
            with self.assertRaises(Unsupported):
                warriors_txd.texture_fields(raw[:size], 0)


if __name__ == '__main__':
    unittest.main()
