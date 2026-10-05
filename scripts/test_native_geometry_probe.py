import struct
import unittest
from native_geometry_probe import Unsupported, dma_probe, native_probe, take

def tag(kind,qwc=0,address=0,vif0=0,vif1=0):
    return struct.pack('<4I',(kind<<28)|qwc,address,vif0,vif1)

def native(body,declared=None):
    return struct.pack('<6I',1,declared if declared is not None else 4+476+len(body),
                       0x1c02000a,4,len(body),0)+body

class NativeTests(unittest.TestCase):
    def test_all_native_truncations(self):
        p=native(tag(6))
        for n in range(len(p)):
            with self.subTest(n=n), self.assertRaises(Unsupported): native_probe(p[:n],100,1)

    def test_bounds(self):
        for off,size in [(-1,1),(0,-1),(2,1),(2**64,1),(0,2**64)]:
            with self.subTest(off=off,size=size),self.assertRaises(Unsupported):take(b'ab',off,size)
        self.assertEqual(take(b'ab',2,0),b'')

    def test_ref_and_integer_source_mapping(self):
        body=tag(3,1,2,0x01000104,0x65028001)+tag(6)+struct.pack('<8h',-3,4,5,-6,0,0,0,0)
        result=dma_probe(body,1000)
        stream=result['unpack_inputs'][0]
        self.assertEqual(stream['raw_vectors'],[[-3,4],[5,-6]])
        self.assertEqual(stream['vector_sources'],[{'offset':1032,'size':4},{'offset':1036,'size':4}])
        self.assertEqual(stream['padding']['size'],8)

    def test_ref_outside_body(self):
        with self.assertRaises(Unsupported):dma_probe(tag(3,1,99),0)

    def test_unknown_tag(self):
        with self.assertRaises(Unsupported):dma_probe(tag(2),0)

    def test_missing_termination(self):
        with self.assertRaises(Unsupported):dma_probe(tag(1),0)

    def test_terminal_inline_bounds(self):
        with self.assertRaises(Unsupported):dma_probe(tag(6,1),0)

    def test_writer_discrepancy_preserved(self):
        r=native_probe(native(tag(6)),123,1)
        self.assertTrue(r['writer_formula_matches'])
        self.assertEqual(r['declared_inner_length']-r['actual_inner_bytes'],468)
        self.assertFalse(r['geometry_decoded'])

    def test_wrong_declared_not_repaired(self):
        r=native_probe(native(tag(6),123),0,1)
        self.assertFalse(r['writer_formula_matches'])
        self.assertEqual(r['declared_inner_length'],123)

    def test_extra_bytes(self):
        with self.assertRaises(Unsupported):native_probe(native(tag(6))+b'!',0,1)

    def test_bad_platform(self):
        p=bytearray(native(tag(6)));struct.pack_into('<I',p,12,5)
        with self.assertRaises(Unsupported):native_probe(p,0,1)

    def test_count_limit(self):
        for count in (0,-1,4097):
            with self.subTest(count=count),self.assertRaises(Unsupported):native_probe(b'',0,count)

    def test_unknown_retained(self):
        r=dma_probe(tag(6)+b'opaque',500)
        self.assertEqual(r['unclassified_ranges'],[{'offset':516,'size':6}])

if __name__=='__main__':unittest.main()
