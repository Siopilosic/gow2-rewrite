"""Corruption checks and independent GS upload-versus-sampling address checks."""
import json
from pathlib import Path
import struct
import unittest

from texture_slice import decode_texture, index_address
from native_geometry_probe import Unsupported, ROOT


def route(values): return sum((v&1)<<i for i,v in enumerate(values))


def gs_address(x,y,bits,width):
    """GS page/block/column addressing in texel units; independent of RW permutation.

    Hardware address bits cross-checked with PCSX2 GSTables.cpp tables.
    """
    if bits==32:
        pw,ph,bw,bh=64,32,8,8
        block=route((x>>3,y>>3,x>>4,y>>4,x>>5))
        col=route((x,y,x>>1,x>>2,y>>1,y>>2))
    elif bits==16:
        pw,ph,bw,bh=64,64,16,8
        block=route((y>>3,x>>4,y>>4,x>>5,y>>5))
        col=route((x>>3,x,y,x>>1,x>>2,y>>1,y>>2))
    elif bits==8:
        pw,ph,bw,bh=128,64,16,16
        block=route((x>>4,y>>4,x>>5,y>>5,x>>6))
        col=route((y>>1,x>>3,x,y,x>>1,(x>>2)^(y>>1)^(y>>2),y>>2,y>>3))
    elif bits==4:
        pw,ph,bw,bh=128,128,32,16
        block=route((y>>4,x>>5,y>>5,x>>6,y>>6))
        col=route((y>>1,x>>3,x>>4,x,y,x>>1,(x>>2)^(y>>1)^(y>>2),y>>2,y>>3))
    else:raise ValueError(bits)
    return ((y//ph)*(width//pw)+x//pw)*(65536//bits)+block*bw*bh+col


class TextureChecks(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        rows=list(struct.iter_unpack('<III',(ROOT/'WARRIORS.DIR').read_bytes()[16:]))
        off,n,_=rows[6871]
        with (ROOT/'WARRIORS.WAD').open('rb') as f:f.seek(off+32);cls.native=f.read(972)

    def test_real_native(self):
        t=decode_texture(self.native)
        self.assertEqual((t['name'],t['width'],t['height'],t['depth']),('metl140_blk_000',32,32,4))
        self.assertEqual(set(t['rgba_gs'][3::4]),{128})

    def test_truncation(self):
        for n in (0,11,43,119,195,207,287,799,970):
            with self.subTest(n=n),self.assertRaises(Unsupported):decode_texture(self.native[:n])

    def test_corrupt_fields(self):
        # Native-relative offsets: width, depth, version, TEX0, payload length, GIF register, image tag.
        for p,value in ((100,0),(108,16),(112,0),(116,0),(164,0),(168,0),(184,0),(216,0xff),(244,0xffffffff)):
            b=bytearray(self.native);struct.pack_into('<I',b,p,value)
            with self.subTest(p=p),self.assertRaises(Unsupported):decode_texture(b)

    def test_independent_gs_addressing(self):
        for depth,w,h in ((4,32,32),(8,64,64),(8,128,128),(4,128,128)):
            upload_bits=16 if depth==4 else 32
            # Upload buffer width is half the sampling TBW, clamped to one 64-pixel unit.
            upload_width=max(64,w//2); sample_width=max(128,w)
            memory={}
            for y in range(h//2):
                for x in range(w//2):
                    dst=gs_address(x,y,upload_bits,upload_width)*upload_bits//depth
                    for lane in range(upload_bits//depth):
                        memory[dst+lane]=(y*(w//2)+x)*(upload_bits//depth)+lane
            for y in range(h):
                for x in range(w):
                    self.assertEqual(memory[gs_address(x,y,depth,sample_width)],index_address(x,y,w))


if __name__=='__main__':unittest.main()
