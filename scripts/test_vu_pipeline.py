import struct
import unittest

from inspect_vu_pipeline import atomic_pipeline, executable_program, extract_upload, upper_annotation, Unsupported
from test_load_map import chunk


def packet(destination=0):
    # Two 64-bit instruction pairs; I flag makes the second lower word data.
    return struct.pack('<8I', 0x60000001, 0, 0, 0x4a020000 | destination,
                       0x8000033c, 0x01cc613c, 0x3f000000, 0x800002ff)


def elf(payload):
    data = bytearray(512)
    data[:7] = b'\x7fELF\x01\x01\x01'
    struct.pack_into('<I', data, 32, 64)
    struct.pack_into('<3H', data, 46, 40, 3, 1)
    names = b'\0.shstrtab\0.vutext\0'
    data[200:200 + len(names)] = names
    struct.pack_into('<10I', data, 104, 1, 3, 0, 0, 200, len(names), 0, 0, 1, 0)
    struct.pack_into('<10I', data, 144, 11, 1, 6, 0x123000, 256, len(payload), 0, 0, 16, 0)
    data[256:256 + len(payload)] = payload
    return data


class PipelineTests(unittest.TestCase):
    def test_exact_source_mapping_and_immediate(self):
        result = extract_upload(packet(), 100)
        self.assertEqual(result['instruction_count'], 2)
        self.assertEqual(result['instructions'][0]['source'], {'offset': 116, 'size': 8})
        self.assertEqual(result['instructions'][0]['upper']['mnemonic'], 'ITOF0')
        self.assertEqual(result['instructions'][0]['upper']['components'], 'xyz')
        self.assertEqual(result['instructions'][1]['lower_role'], 'IMMEDIATE_BITS')
        self.assertFalse(result['render_ready'])

    def test_every_packet_truncation_and_trailing_bytes(self):
        for n in range(len(packet())):
            with self.subTest(n=n), self.assertRaises(Unsupported):
                extract_upload(packet()[:n])
        with self.assertRaises(Unsupported):
            extract_upload(packet() + bytes(16))

    def test_zero_count_means_256(self):
        data = struct.pack('<4I', 0x60000080, 0, 0, 0x4a000000) + bytes(2048)
        self.assertEqual(extract_upload(data)['instruction_count'], 256)

    def test_destination_limit(self):
        self.assertEqual(extract_upload(packet(2046))['instructions'][-1]['pc'], 2047)
        with self.assertRaises(Unsupported):
            extract_upload(packet(2047))

    def test_unsupported_tag_and_command(self):
        for offset, value in ((0, 0x50000001), (4, 16), (12, 0x14000000)):
            data = bytearray(packet()); struct.pack_into('<I', data, offset, value)
            with self.subTest(offset=offset), self.assertRaises(Unsupported):
                extract_upload(data)

    def test_overlapping_uploads(self):
        data = struct.pack('<4I', 0x60000002, 0, 0, 0x4a010000) + bytes(8)
        data += struct.pack('<2I', 0, 0x4a010000) + bytes(16)
        with self.assertRaisesRegex(Unsupported, 'overlapping'):
            extract_upload(data)

    def test_conversion_scale_and_unknown(self):
        for opcode, name in ((60, 'ITOF0'), (61, 'ITOF4'), (62, 'ITOF12'), (63, 'ITOF15')):
            self.assertEqual(upper_annotation((4 << 6) | opcode)['mnemonic'], name)
        self.assertEqual(upper_annotation(0)['mnemonic'], 'UNDECODED')

    def test_elf_mapping_not_fixed_delta(self):
        r = executable_program(elf(packet()), 0x123000)
        self.assertEqual(r['source']['offset'], 256)
        self.assertEqual(r['instructions'][0]['virtual_address'], 0x123010)
        for va in (0, 0x122fff, 0x123020):
            with self.subTest(va=va), self.assertRaises(Unsupported):
                executable_program(elf(packet()), va)

    def test_elf_section_boundary(self):
        data = elf(packet()); struct.pack_into('<I', data, 164, 16)
        with self.assertRaises(Unsupported): executable_program(data, 0x123000)

    def test_elf_truncations(self):
        data = elf(packet())
        for n in range(288):
            with self.subTest(n=n), self.assertRaises(Unsupported):
                executable_program(data[:n], 0x123000)

    def test_plugin_identity_and_absence(self):
        plugin = chunk(31, struct.pack('<2I', 3, 0x30083))
        r = atomic_pipeline(chunk(3, plugin), 100)
        self.assertEqual(r[0]['pipeline_id'], 0x30083)
        self.assertEqual(r[0]['source']['offset'], 124)
        self.assertEqual(atomic_pipeline(chunk(3, b''), 0), [])
        with self.assertRaises(Unsupported): atomic_pipeline(chunk(3, plugin + plugin), 0)
        with self.assertRaises(Unsupported): atomic_pipeline(chunk(3, chunk(31, b'')), 0)


if __name__ == '__main__': unittest.main()
