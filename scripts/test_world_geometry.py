import struct
import unittest
from inspect_world_geometry import inspect_geometry,Unsupported
from test_load_map import chunk
from test_native_geometry_probe import native,tag

def geometry():
    return chunk(15,chunk(1,struct.pack('<4I',0x01010037,2,4,1))+
                 chunk(3,chunk(0x50e,struct.pack('<5I',1,1,4,4,0))+chunk(0x510,native(tag(6)))))

class GeometryTests(unittest.TestCase):
    def test_source_and_counts(self):
        g=inspect_geometry(geometry(),100)
        self.assertEqual(g['geometry_struct_source'],{'offset':124,'size':16})
        self.assertEqual(g['mesh_count'],1)
        self.assertTrue(g['writer_formula_matches'])
        self.assertEqual(g['meshes'][0]['tag_count'],1)
        self.assertFalse(g['geometry_decoded'])

    def test_every_truncation(self):
        data=geometry()
        for n in range(len(data)):
            with self.subTest(n=n),self.assertRaises(Unsupported):inspect_geometry(data[:n],0)

    def test_missing_extension(self):
        with self.assertRaises(Unsupported):inspect_geometry(chunk(15,chunk(1,bytes(16))),0)

    def test_duplicate_native(self):
        b=chunk(0x50e,struct.pack('<3I',1,1,4))+chunk(0x510,native(tag(6)))*2
        with self.assertRaises(Unsupported):inspect_geometry(chunk(15,chunk(1,bytes(16))+chunk(3,b)),0)

if __name__=='__main__':unittest.main()
