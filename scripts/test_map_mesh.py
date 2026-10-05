import struct
import unittest
from export_map_mesh import decode_atomic, Unsupported
from test_load_map import chunk


def fixture(w=0, count=2, scale=1., radius=1.):
    # Strip quad in XY. Each REF targets an explicitly separated attribute array.
    inputs = [struct.pack('<16h', 0,0,0,0, 1,0,0,0, 0,1,0,0, 1,1,0,w), bytes(16), bytes(16), bytes(16)]
    commands = [0x6d048000, 0x65048001, 0x6e04c002, 0x6e048003]
    offset = 96; tags = b''
    for data, command in zip(inputs, commands):
        tags += struct.pack('<4I', 0x30000000+len(data)//16, offset//16, 0x01000104, command)
        offset += len(data)
    body = tags+struct.pack('<8I',0x60000001,0,0,0,0x04000004,0x15000000,0x11000000,0x11000000)+b''.join(inputs)
    native = struct.pack('<6I',1,4+476+len(body),0x1c02000a,4,len(body),0)+body
    header = struct.pack('<4I4f2I',0x0101003d,count,4,1,.5,.5,0,radius,0,0)
    materials = chunk(8,chunk(1,struct.pack('<2i',1,-1))+chunk(7,chunk(1,bytes(28))+chunk(3,b'')))
    geo = chunk(15,chunk(1,header)+materials+chunk(3,chunk(0x50e,struct.pack('<5I',1,1,4,4,0))+chunk(0x510,native)))
    ext = chunk(3,chunk(31,struct.pack('<2I',3,0x30083))+chunk(0x3f0,struct.pack('<ffI',scale,1.,0)))
    return geo, ext


class MeshTests(unittest.TestCase):
    def test_quad_topology_and_exact_provenance(self):
        g,e=fixture();r=decode_atomic(g,e,1000,2000)
        self.assertEqual(r['triangles'], [[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]], [[0.,1.,0.],[1.,0.,0.],[1.,1.,0.]]])
        for tri,source in zip(r['triangles'],r['primitive_sources']):
            for v,o in zip(tri,source['vertex_offsets']):
                self.assertEqual(v,list(struct.unpack_from('<3h',g,o-1000)))

    def test_every_geometry_and_extension_truncation(self):
        g,e=fixture()
        for n in range(len(g)):
            with self.subTest(n=n),self.assertRaises(Unsupported):decode_atomic(g[:n],e,0,0)
        for n in range(len(e)):
            with self.subTest(n=n),self.assertRaises(Unsupported):decode_atomic(g,e[:n],0,0)

    def test_adc_not_silently_drawn(self):
        with self.assertRaisesRegex(Unsupported,'ADC'):decode_atomic(*fixture(w=-32768),0,0)

    def test_triangle_count_mismatch_withheld(self):
        with self.assertRaisesRegex(Unsupported,'triangle count'):decode_atomic(*fixture(count=3),0,0)

    def test_zero_area_draw_kept_as_diagnostic(self):
        g,e=fixture()
        pattern=struct.pack('<16h',0,0,0,0,1,0,0,0,0,1,0,0,1,1,0,0)
        offset=g.index(pattern);g=bytearray(g)
        struct.pack_into('<4h',g,offset+24,0,1,0,0)
        r=decode_atomic(g,e,0,0)
        self.assertEqual(len(r['triangles']),1)
        self.assertEqual(r['strip_triangle_count'],2)
        self.assertEqual(r['stored_visible_count_difference'],1)
        self.assertEqual(r['zero_area_strip_triangles'],1)

    def test_wrong_scale_rejected_by_independent_sphere(self):
        with self.assertRaisesRegex(Unsupported,'bounding sphere'):decode_atomic(*fixture(scale=100.,radius=.1),0,0)

    def test_nonfinite_scale(self):
        with self.assertRaises(Unsupported):decode_atomic(*fixture(scale=float('nan')),0,0)


if __name__=='__main__':unittest.main()
