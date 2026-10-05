"""Verify the complete menu library against original packed vertex bytes."""
import hashlib
import json
import mmap
from pathlib import Path
import struct
import subprocess
from load_map import ROOT


def main():
    folder=ROOT/'research/reports/map-library'
    library=json.loads((folder/'library.json').read_text())
    viewer=ROOT/'research/native/build/Release/WarriorsMapViewer.exe'
    vertices=0;triangles=0;parts=0
    with (ROOT/'WARRIORS.WAD').open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as wad:
        for entry in library['maps']:
            path=folder/entry['level'];r=json.loads((path/'mesh.json').read_text());data=(path/'map.wmv').read_bytes()
            magic,nv,np,skipped=struct.unpack_from('<4sIII',data)
            assert magic==b'WMV1' and nv==entry['triangles']*3 and np==entry['parts'] and skipped==entry['skipped']
            at=16+20*np
            for i,item in enumerate(r['items']):
                assert struct.unpack_from('<5I',data,16+20*i)==(item['triangle_start']*3,item['triangle_count']*3,item['part_member'],item['slot'],item['world'])
                scale=struct.unpack_from('<f',wad,item['scale_source']['offset'])[0]
                translation=struct.unpack_from('<3f',wad,item['translation_source']['offset'])
                assert scale==item['position_scale'] and list(translation)==item['translation']
                assert item['triangle_count']==len(item['primitive_sources'])
                assert item['triangle_count']<=item['stored_triangle_count']<=item['strip_triangle_count']
                for primitive in item['primitive_sources']:
                    for offset in primitive['vertex_offsets']:
                        x,y,z,w=struct.unpack_from('<4h',wad,offset);assert w==0
                        expected=struct.pack('<3f',x*scale+translation[0],y*scale+translation[1],z*scale+translation[2])
                        assert data[at:at+12]==expected;at+=12;vertices+=1
            assert at==len(data)
            run=subprocess.run([str(viewer),str(path/'map.wmv'),'--validate'],capture_output=True,text=True)
            assert run.returncode==0,run.stderr
            triangles+=nv//3;parts+=np
            print(entry['level']+' verified',flush=True)
    malformed=folder/'catalog-tests';malformed.mkdir()
    cases=['BAD\n','WMVCATALOG1\nlevel1\t../outside.wmv\t1\t1\t0\t0\n',
           'WMVCATALOG1\nlevel1\tC:/outside.wmv\t1\t1\t0\t0\n','WMVCATALOG1\n',
           'WMVCATALOG1\n<script>\tmap.wmv\t1\t1\t0\t0\n']
    for i,case in enumerate(cases):
        p=malformed/(str(i)+'.tsv');p.write_text(case)
        run=subprocess.run([str(viewer),'--catalog',str(p),'--validate'],capture_output=True,text=True)
        assert run.returncode==1,(i,run.stdout,run.stderr)
    hashes={}
    for name,expected in library['source_identities'].items():
        with (ROOT/name).open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
        assert actual==expected;hashes[name]=actual
    report={'maps_verified':len(library['maps']),'map_parts':parts,'triangles':triangles,
            'independent_vertex_byte_comparisons':vertices,'unavailable_parts':sum(m['skipped']for m in library['maps']),
            'catalog_invalid_inputs_rejected':len(cases),'source_identities_unchanged':hashes,
            'one_to_one_verified':False,'textures_decoded':False}
    with (folder/'verification.json').open('x') as f:json.dump(report,f,indent=2)
    print(json.dumps(report))


if __name__=='__main__':main()
