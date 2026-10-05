"""Independent checks of native reports against source bytes and Python standard library."""
from pathlib import Path
import collections
import hashlib
import json
import struct
import subprocess
import sys

root=Path(__file__).resolve().parents[2]
out=Path(sys.argv[1]).resolve()
manifest=json.loads((root/'research/reports/initial/manifest.json').read_text())
checks=0
def check(condition,message):
    global checks
    assert condition,message
    checks+=1
hash_results=[]
for f in manifest['files']:
    with (root/f['path']).open('rb') as stream: actual=hashlib.file_digest(stream,'sha256').hexdigest()
    check(actual==f['sha256'],'Original source changed: '+f['path'])
    hash_results.append({'path':f['path'],'sha256':actual,'unchanged':True})
records=json.loads((out/'dir-records.json').read_text())['items']
members=json.loads((out/'wad-members.json').read_text())['items']
python_resources=json.loads((out/'resource-catalogue.json').read_text())['items']
directory=(root/'WARRIORS.DIR').read_bytes()
rows=list(struct.iter_unpack('<III',directory[16:]))
check(len(rows)==len(records)==len(members)==len(python_resources),'Coverage mismatch')
with (root/'WARRIORS.WAD').open('rb') as wad:
    for i,(offset,length,key) in enumerate(rows):
        rec=records[i];member=members[i]
        check((rec['word0'],rec['word1'],rec['word2'])==(offset,length,key),'Record mismatch')
        wad.seek(offset);header=wad.read(min(length,64))
        check(member['first_bytes']==header.hex(' '),'Header mismatch')
        check(member['linear_resources']['status']==python_resources[i]['status'],'Independent resource layout mismatch')
        for c in member['chunks']['candidate_chunks']:
            span=c['source'];check(offset<=span['offset'] and span['offset']+span['size']<=offset+length,'Unbounded chunk')
native=root/'research/native/build/Release/WarriorsMapTool.exe'
for cmd,args in [('archive-info',[]),('archive-list',[]),('member-info',['201']),('member-dump',['201']),('member-strings',['6868']),('member-hex',['201','0','32'])]:
    p=subprocess.run([str(native),cmd,str(root/'WARRIORS.DIR'),str(root/'WARRIORS.WAD'),*args],capture_output=True,text=True)
    check(p.returncode==0,cmd+': '+p.stderr);json.loads(p.stdout)
    if cmd=='member-dump':(out/'geometry-candidate-201.json').write_text(p.stdout)
unit=subprocess.run([str(root/'research/native/build/Release/WarriorsTests.exe')],cwd=root/'research/native/build',capture_output=True,text=True,check=True)
cli=subprocess.run([sys.executable,str(root/'research/native/tests/integration.py')],capture_output=True,text=True,check=True)
result={'native_unit_result':unit.stdout.strip(),'cli_tests':json.loads(cli.stdout),'independent_checks_passed':checks,'original_hashes':hash_results,'reference_execution':'NOT_RUN','geometry_decoded':False}
(out/'verification.json').write_text(json.dumps(result,indent=2))
print(json.dumps({k:v for k,v in result.items() if k!='original_hashes'},indent=2))
