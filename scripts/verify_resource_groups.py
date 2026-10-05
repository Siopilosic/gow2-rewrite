"""Independent source checks for the native grouped-resource catalogue."""
import collections
import hashlib
import json
import mmap
from pathlib import Path
import struct
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
out=ROOT/'research/reports/resource-groups-native';out.mkdir(exist_ok=True)
exe=ROOT/'research/native/build/Release/WarriorsMapTool.exe'
with (out/'groups.json').open('w') as f:
    subprocess.run([str(exe),'resource-groups',str(ROOT/'WARRIORS.DIR'),str(ROOT/'WARRIORS.WAD'),str(ROOT/'SLUS_212.15')],stdout=f,check=True)
report=json.loads((out/'groups.json').read_text())
rows=list(struct.iter_unpack('<III',(ROOT/'WARRIORS.DIR').read_bytes()[16:]))
actual={x['member_index']:x for x in report['members']};expected={};groups=blocks=0
tokens=collections.defaultdict(list)
with (ROOT/'WARRIORS.WAD').open('rb') as f:
    with mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as wad:
        for member,(base,size,key) in enumerate(rows):
            if size<32:continue
            outer=struct.unpack_from('<4I',wad,base);cursor=16;count=0;parsed=[]
            if not 0<outer[0]<=4096:continue
            try:
                for ordinal in range(outer[0]):
                    if cursor+16>size:raise ValueError()
                    location=cursor;g=struct.unpack_from('<4I',wad,base+cursor);cursor+=16
                    if not 0<g[0]<=4096 or g[3]==0:raise ValueError()
                    entries=[]
                    for j in range(g[0]):
                        count+=1
                        if count>100000 or cursor+16>size:raise ValueError()
                        at=cursor;v=struct.unpack_from('<4I',wad,base+cursor);cursor+=16
                        if v[0]>=84 or v[1]>size-cursor or v[1]%4:raise ValueError()
                        entries.append((at,v));cursor+=v[1]
                    parsed.append((location,g,entries))
                if cursor!=size:raise ValueError()
            except ValueError:continue
            expected[member]=len(parsed);n=actual[member]
            assert n['header_words']==list(outer) and n['source']['offset']==base and n['source']['size']==size
            assert len(n['groups'])==len(parsed)
            for ordinal,((location,g,entries),ng) in enumerate(zip(parsed,n['groups'])):
                assert ng['raw_words']==list(g) and ng['header_source']['offset']==base+location
                assert ng['lookup_token']==g[3] and ng['condition_status'].startswith('UNRESOLVED')
                assert len(ng['blocks'])==len(entries)
                groups+=1
                tokens[f'0x{g[3]:08x}'].append({'member_index':member,'group_ordinal':ordinal,'header_offset':base+location})
                for (at,v),nb in zip(entries,ng['blocks']):
                    assert nb['raw_words']==list(v)
                    assert nb['header_source']['offset']==base+at and nb['header_source']['size']==16
                    assert nb['payload_source']['offset']==base+at+16 and nb['payload_source']['size']==v[1]
                    blocks+=1
assert set(actual)==set(expected)
assert groups==report['group_count'] and blocks==report['resource_count']
for name,expected_hash in (('SLUS_212.15',report['build_id']),):
    with (ROOT/name).open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==expected_hash
for start,end in ((0x144398,0x1446b8),(0x186110,0x186240),(0x187c38,0x187d20),(0x40e4c4,0x40e528),(0x40d974,0x40d9bc),(0x40dac4,0x40db08)):
    text=subprocess.check_output([sys.executable,str(ROOT/'research/scripts/loader_probe.py'),hex(start),hex(end)],text=True)
    (out/f'evidence-{start:08x}.asm.txt').write_text(text)
(out/'token-candidates.json').write_text(json.dumps({'semantics':'Group lookup tokens; candidate locations, not resolved archive dependencies','tokens':tokens},separators=(',',':')))
summary={'members_examined':len(rows),'grouped_members':len(actual),'group_headers_source_checked':groups,
         'resource_records_source_checked':blocks,'unique_tokens':len(tokens),
         'tokens_with_multiple_locations':sum(len(v)>1 for v in tokens.values()),
         'independent_container_scan':'PASS','category_complete':False,
         'runtime_selection':'UNRESOLVED: do not load every group unconditionally'}
(out/'validation.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
