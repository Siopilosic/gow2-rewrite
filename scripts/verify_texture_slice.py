"""Verify slice provenance, GS texture addressing, native input rejection and originals."""
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import unittest

from native_geometry_probe import ROOT
from test_texture_slice import TextureChecks

out=Path(sys.argv[1]).resolve()
r=json.loads((out/'scene.warriors.json').read_text())
result=unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(TextureChecks))
assert result.wasSuccessful()
checks=0
with (ROOT/'WARRIORS.WAD').open('rb') as f:
    for name,span in r['native_sources'].items():
        filename={'dictionary':'dictionary.bin','geometry':'geometry.bin','extension':'atomic-extension.bin'}[name]
        f.seek(span['offset']);assert f.read(span['size'])==(out/'native-state'/filename).read_bytes()
    for t in r['textures'].values():
        raw=(out/'textures'/f'{t["name"]}.gs.rgba').read_bytes()
        indices=(out/'textures'/f'{t["name"]}.indices').read_bytes()
        for i,(offset,shift,pal) in enumerate(t['texel_sources']):
            f.seek(offset);value=f.read(1)[0]
            index=(value>>shift)&((1<<t['depth'])-1)
            assert indices[i]==index
            expected_index=(index & ~24)|((index&8)<<1)|((index&16)>>1) if t['depth']==8 else index
            assert pal==t['palette_transfer']['pixels_source']['offset']+expected_index*4
            f.seek(pal);assert f.read(4)==raw[i*4:i*4+4];checks+=1
    for c in r['corners']:
        for lane,span in c['sources'].items():
            f.seek(span['offset']);values=list(struct.unpack({'0':'<4h','1':'<2h','2':'<4B','3':'<4b'}[lane],f.read(span['size'])))
            assert values==c['raw_inputs'][lane]
        assert c['uv']==[x*r['uv_scale'] for x in c['raw_inputs']['1']]

exe=ROOT/'research/native/build/Release/WarriorsMapViewer.exe'
blob=(out/'material.wmv').read_bytes();_,nv,np,_=struct.unpack_from('<4sIII',blob)
assert nv==len(r['corners']) and np==len(r['materials'])
for i,c in enumerate(r['corners']):
    assert blob[16+20*np+12*i:16+20*np+12*(i+1)]==struct.pack('<3f',*c['position'])
    uv=16+20*np+12*nv+8*i
    assert blob[uv:uv+8]==struct.pack('<2f',*c['uv'])
subprocess.run([str(exe),str(out/'material.wmv'),'--validate'],check=True)
bad=[b'',blob[:15],blob[:-1],blob+b'x']
for offset,value in ((16+20*np+12*nv,0x7fc00000),(16+20*np+20*nv,0xffffffff),(16,nv),(4,0xffffffff)):
    b=bytearray(blob);struct.pack_into('<I',b,offset,value);bad.append(b)
malformed=out/'malformed';malformed.mkdir(exist_ok=True)
for i,b in enumerate(bad):
    p=malformed/f'{i}.wmv';p.write_bytes(b)
    v=subprocess.run([str(exe),str(p),'--validate'],capture_output=True)
    assert v.returncode==1,(i,v.stdout,v.stderr)
originals=json.loads((ROOT/'research/reports/archive-analysis-verified/verification.json').read_text())['original_hashes']
for item in originals:
    with (ROOT/item['path']).open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==item['sha256']
summary={'texel_source_comparisons':checks,'corner_attribute_source_comparisons':len(r['corners'])*4,
         'native_malformed_rejections':len(bad),'original_files_unchanged':len(originals),
         'independent_gs_addressing_tests':'PASS','reference_render_matched':False}
(out/'source-validation.json').write_text(json.dumps(summary,indent=2))
print(json.dumps(summary))
