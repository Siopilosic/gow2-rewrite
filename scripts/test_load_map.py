import io
import struct
import unittest
import zlib
from collections import defaultdict
from load_map import world_layout, SceneLoader, Unsupported

def chunk(t,b):return struct.pack('<III',t,len(b),0x1c02000a)+b
def world(slot=7,group=1,xyz=(1.25,-2.5,3.0)):
    plugin=chunk(0x3f1,struct.pack('<II3f',slot,group,*xyz))
    return struct.pack('<I',1)+chunk(22,b'')+chunk(11,chunk(9,chunk(3,plugin)))

class WorldTests(unittest.TestCase):
    def test_translation_provenance(self):
        r=world_layout(world(),1000);s=r['slots'][0]
        self.assertEqual(s['translation'],[1.25,-2.5,3.0])
        self.assertEqual(s['translation_source'],{'offset':1072,'size':12})
        self.assertEqual(struct.unpack_from('<3f',world(),72),tuple(s['translation']))
        self.assertEqual(s['group'],1)

    def test_all_truncations(self):
        data=world()
        for n in range(len(data)):
            with self.subTest(n=n),self.assertRaises(Unsupported):world_layout(data[:n])

    def test_invalid_group(self):
        with self.assertRaises(Unsupported):world_layout(world(group=2))

    def test_inactive_slot(self):
        self.assertFalse(world_layout(world(slot=0xffffffff,group=0))['slots'][0]['active'])

    def test_nan(self):
        with self.assertRaises(Unsupported):world_layout(world(xyz=(float('nan'),0,0)))

    def test_duplicate_slot(self):
        sector=chunk(9,chunk(3,chunk(0x3f1,struct.pack('<II3f',7,1,0,0,0))))
        with self.assertRaises(Unsupported):world_layout(struct.pack('<I',1)+chunk(22,b'')+chunk(11,sector*2))

    def test_trailing_bytes(self):
        with self.assertRaises(Unsupported):world_layout(world()+b'!')

    def test_scene_resolves_and_checks_membership(self):
        loader=SceneLoader.__new__(SceneLoader);data=world();loader.wad=io.BytesIO(data)
        names=['./ee_files/test_sec.wld','./ee_files/test_ms1.sec']
        loader.rows=[(0,len(data),zlib.crc32(names[0].encode())),(100,40,zlib.crc32(names[1].encode()))]
        loader.keys=defaultdict(list,{r[2]:[i] for i,r in enumerate(loader.rows)})
        loader.identities={'WARRIORS.WAD':'synthetic'}
        loader.parts={1:{'texture_dictionary':{},'atomic_count':1,'atomics':[{'sector_slot':7,'source':{},'geometry':{}}]}}
        scene=loader.load_world('test')
        self.assertEqual(scene['instances'][0]['translation'],[1.25,-2.5,3.0])
        loader.parts[1]['atomics'][0]['sector_slot']=8
        with self.assertRaises(Unsupported):loader.load_world('test')
        loader.keys[loader.rows[1][2]].append(2)
        with self.assertRaises(Unsupported):loader.lookup(names[1])

if __name__=='__main__':unittest.main()
