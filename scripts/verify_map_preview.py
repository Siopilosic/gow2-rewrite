"""Independently compare displayed float32 vertices with original packed bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]


def main():
    p=argparse.ArgumentParser();p.add_argument('directory',type=Path);args=p.parse_args();out=args.directory
    report=json.loads((out/'mesh.json').read_text());blob=(out/'map.wmv').read_bytes()
    magic,nv,np,skipped=struct.unpack_from('<4sIII',blob)
    assert magic==b'WMV1' and nv==report['triangle_count']*3 and np==len(report['items']) and skipped==len(report['skipped_atomics'])
    cursor=16+20*np; checks=0
    with (ROOT/'WARRIORS.WAD').open('rb') as wad:
        for i,item in enumerate(report['items']):
            assert struct.unpack_from('<5I',blob,16+20*i)==(item['triangle_start']*3,item['triangle_count']*3,item['part_member'],item['slot'],item['world'])
            assert item['triangle_count']==len(item['primitive_sources'])
            assert item['triangle_count']<=item['stored_triangle_count']<=item.get('strip_triangle_count',item['stored_triangle_count'])
            if 'stored_visible_count_difference' in item:
                assert item['stored_visible_count_difference']==item['stored_triangle_count']-item['triangle_count']
            wad.seek(item['scale_source']['offset']);scale=struct.unpack('<f',wad.read(4))[0]
            assert scale==item['position_scale']
            wad.seek(item['translation_source']['offset']);translation=struct.unpack('<3f',wad.read(12))
            assert list(translation)==item['translation']
            for primitive in item['primitive_sources']:
                for offset in primitive['vertex_offsets']:
                    wad.seek(offset);raw=struct.unpack('<4h',wad.read(8));assert raw[3]==0
                    expected=struct.pack('<3f',*(raw[k]*scale+translation[k] for k in range(3)))
                    assert blob[cursor:cursor+12]==expected;cursor+=12;checks+=1
    assert cursor==len(blob)
    exe=ROOT/'research/native/build/Release/WarriorsMapViewer.exe'
    good=subprocess.run([str(exe),str(out/'map.wmv'),'--validate'],capture_output=True,text=True)
    assert good.returncode==0,good.stderr
    malformed=out/'malformed';malformed.mkdir()
    mutations=[b'',blob[:15],blob[:-1],blob+b'x',b'BAD!'+blob[4:]]
    bad=bytearray(blob);struct.pack_into('<f',bad,16+20*np,float('nan'));mutations.append(bad)
    bad=bytearray(blob);struct.pack_into('<I',bad,16,nv);mutations.append(bad)
    for i,b in enumerate(mutations):
        path=malformed/(str(i)+'.wmv');path.write_bytes(b)
        r=subprocess.run([str(exe),str(path),'--validate'],capture_output=True,text=True)
        assert r.returncode==1,(i,r.stdout,r.stderr)
    original=json.loads((ROOT/'research/reports/archive-analysis-verified/verification.json').read_text())['original_hashes']
    identities=[]
    for item in original:
        with (ROOT/item['path']).open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
        assert actual==item['sha256'];identities.append({'path':item['path'],'sha256':actual,'unchanged':True})
    for start,end in ((0x192618,0x192908),(0x429288,0x42930c),(0x429788,0x429844),(0x4298c0,0x429a60),(0x4273f8,0x4275d0),(0x4278b0,0x4279e8)):
        r=subprocess.run([sys.executable,str(ROOT/'research/scripts/loader_probe.py'),hex(start),hex(end)],capture_output=True,text=True,check=True)
        with (out/('evidence-%08x.asm.txt'%start)).open('x') as f:f.write(r.stdout)
    verification={'independent_vertex_source_checks':checks,'native_preview_validated':True,'malformed_preview_rejections':len(mutations),
                  'original_files':identities,'one_to_one_verified':False}
    with (out/'verification.json').open('x') as f:json.dump(verification,f,indent=2)
    print(json.dumps({k:v for k,v in verification.items() if k!='original_files'}))


if __name__=='__main__':main()
