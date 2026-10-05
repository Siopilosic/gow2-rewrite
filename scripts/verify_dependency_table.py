"""Validate the native dependency reader and bounded initializer-state inference."""
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib

ROOT=Path(__file__).resolve().parents[2]
out=ROOT/'research/reports/dependency-table-native';out.mkdir(exist_ok=True)
run=subprocess.run([str(ROOT/'research/native/build/Release/WarriorsMapTool.exe'),'dependency-table',
                    str(ROOT/'WARRIORS.DIR'),str(ROOT/'WARRIORS.WAD'),str(ROOT/'SLUS_212.15')],capture_output=True,text=True,check=True)
data=json.loads(run.stdout);span=data['source']
with (ROOT/'WARRIORS.WAD').open('rb') as f:f.seek(span['offset']);raw=f.read(span['size'])
count=struct.unpack_from('<I',raw)[0];assert count==data['record_count']
expected=list(struct.iter_unpack('<IIHH',raw[16:16+count*12]))
token=zlib.crc32(b'always');assert token==data['initializer']['token']
candidates=[]
for ordinal,(words,record) in enumerate(zip(expected,data['records'])):
    assert words==tuple(record[k] for k in ('key0','key1','stored_state0','stored_state1'))
    assert record['source']['offset']==span['offset']+16+ordinal*12 and record['source']['size']==12
    assert bytes.fromhex(record['raw_hex'])==raw[16+ordinal*12:28+ordinal*12]
    # Original 0x178bc8 applies a supplied state to both matching key columns.
    s0=1 if words[0]==token else words[2];s1=1 if words[1]==token else words[3]
    if s1==1 and s0!=1:candidates.append({'record_ordinal':ordinal,'key0':words[0]})
assert len(data['records'])==len(expected)
scenario=data['activation_scenario']
assert scenario['events']==[]
assert [(r['record_ordinal'],r['key0']) for r in scenario['candidate_rows']]==[(r['record_ordinal'],r['key0']) for r in candidates]
assert scenario['next_record_ordinal']==(candidates[-1]['record_ordinal'] if candidates and candidates[-1]['key0'] else None)
assert data['opaque_tail']['offset']==span['offset']+16+count*12
assert data['opaque_tail']['size']==len(raw)-16-count*12
objects=data['object_list'];os=objects['source']
with (ROOT/'WARRIORS.WAD').open('rb') as f:f.seek(os['offset']);ob=f.read(os['size'])
on=struct.unpack_from('<I',ob)[0];assert on==len(objects['records'])
bindings={}
for ordinal,words in enumerate(struct.iter_unpack('<9I',ob[16:16+on*36])):
    record=objects['records'][ordinal]
    assert list(words)==record['raw_words'] and record['source']['offset']==os['offset']+16+ordinal*36
    for field in (2,3,4):bindings.setdefault(words[field],[]).append((ordinal,field*4,os['offset']+16+ordinal*36+field*4))
for match in data['object_token_matches']:
    expected_matches=bindings.get(match['key0'],[])
    assert expected_matches==[(c['object_ordinal'],c['record_field_offset'],c['source']['offset']) for c in match['candidates']]
    choice=match['static_lookup_first_match']
    if match['key0'] and expected_matches:
        assert (choice['object_ordinal'],choice['record_field_offset'],choice['source']['offset'])==expected_matches[0]
    else:assert choice is None
assert data['unmatched_object_tokens']==sum(not bindings.get(x['key0']) for x in data['object_token_matches'])
assert len(data['object_token_matches'])==len({r[0] for r in expected})
assert data['ambiguous_object_tokens']==sum(len(bindings.get(x['key0'],[]))>1 for x in data['object_token_matches'])
startup=data['startup_container'];ss=startup['source']
elf=(ROOT/'SLUS_212.15').read_bytes();ls=startup['literal_source']
assert elf[ls['offset']:ls['offset']+ls['size']]==b'global.pak\0'
directory=(ROOT/'WARRIORS.DIR').read_bytes();key=zlib.crc32(b'./ee_files/global.pak')
archive_rows=list(struct.iter_unpack('<III',directory[16:]))
format_span=data['object_filename_format_source']
assert elf[format_span['offset']:format_span['offset']+format_span['size']]==b'%u\0'
assert {r['key0'] for r in data['object_resource_locations']}=={r[0] for r in expected}
resolved_count=0;resource_types={}
with (ROOT/'WARRIORS.WAD').open('rb') as f:
    for resource in data['object_resource_locations']:
        name=f'./ee_files/{resource["key0"]}';assert name==resource['lookup_name']
        matches=[i for i,r in enumerate(archive_rows) if r[2]==zlib.crc32(name.encode())]
        assert matches==[t['member_index'] for t in resource['targets']]
        if len(matches)!=1:continue
        resolved_count+=1;t=resource['targets'][0];base,size,_=archive_rows[matches[0]]
        assert (t['source']['offset'],t['source']['size'])==(base,size)
        ds=t['dir_record_source'];assert (ds['offset'],ds['size'])==(16+matches[0]*12,12)
        assert bytes.fromhex(t['dir_record_hex'])==directory[ds['offset']:ds['offset']+12]
        f.seek(base);b=f.read(size);n=struct.unpack_from('<I',b)[0];p=16
        c=t['container'];assert c['layout']=='LINEAR_RESOURCE_CONTAINER'
        blocks=c['groups'][0]['blocks'];assert n==len(blocks)
        for block in blocks:
            words=struct.unpack_from('<4I',b,p);assert list(words)==block['raw_words']
            assert (block['header_source']['offset'],block['header_source']['size'])==(base+p,16)
            assert (block['payload_source']['offset'],block['payload_source']['size'])==(base+p+16,words[1])
            resource_types[words[0]]=resource_types.get(words[0],0)+1;p+=16+words[1];assert p<=len(b)
        assert p==len(b)
assert resolved_count==data['resolved_object_resources']
resolved=[(i,r) for i,r in enumerate(struct.iter_unpack('<III',directory[16:])) if r[2]==key]
assert len(resolved)==1 and resolved[0][0]==startup['member_index']
assert resolved[0][1][:2]==(ss['offset'],ss['size'])
ds=startup['dir_record_source'];assert ds['offset']==16+12*startup['member_index'] and ds['size']==12
assert bytes.fromhex(startup['dir_record_hex'])==directory[ds['offset']:ds['offset']+12]
with (ROOT/'WARRIORS.WAD').open('rb') as f:f.seek(ss['offset']);sb=f.read(ss['size'])
group_count=struct.unpack_from('<I',sb)[0];pos=16;scoped={}
for ordinal in range(group_count):
    n,_,_,group_token=struct.unpack_from('<4I',sb,pos)
    scoped.setdefault(group_token,[]).append((ordinal,ss['offset']+pos+12));pos+=16
    for _ in range(n):
        length=struct.unpack_from('<I',sb,pos+4)[0];pos+=16+length
        assert pos<=len(sb)
assert pos==len(sb) and group_count==len(startup['groups'])
assert {m['key0'] for m in data['startup_group_matches']}=={r[0] for r in expected}
for match in data['startup_group_matches']:
    assert [(c['group_ordinal'],c['source']['offset']) for c in match['candidates']]==scoped.get(match['key0'],[])
    assert all(c['source']['size']==4 for c in match['candidates'])
index=json.loads((ROOT/'research/reports/resource-groups-native/token-candidates.json').read_text())['tokens']
for candidate in candidates:
    candidate['group_locations']=index.get(f'0x{candidate["key0"]:08x}',[])
    candidate['startup_group_ordinals']=[ordinal for ordinal,_ in scoped.get(candidate['key0'],[])]
inference={'scope':'One application of sourced always initializer; not runtime execution or full closure',
           'token':token,'candidate_rows':candidates,'next_candidate_key':candidates[-1]['key0'] if candidates else None,
           'archive_selection':'UNRESOLVED; multiple group locations must not be arbitrarily selected'}
(out/'initializer-inference.json').write_text(json.dumps(inference,indent=2))
originals=json.loads((ROOT/'research/reports/archive-analysis-verified/verification.json').read_text())['original_hashes']
for source in originals:
    with (ROOT/source['path']).open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==source['sha256']
for start,end in ((0x178b30,0x178ee8),(0x181170,0x18123c),(0x1849e0,0x184a2c),(0x1897f4,0x18986c),(0x187aa8,0x187c38),(0x154cf8,0x154d1c),(0x1866e8,0x1868b0)):
    text=subprocess.check_output([sys.executable,str(ROOT/'research/scripts/loader_probe.py'),hex(start),hex(end)],text=True)
    (out/f'evidence-{start:08x}.asm.txt').write_text(text)
(out/'dependencies.json').write_text(run.stdout)
level_run=subprocess.run([str(ROOT/'research/native/build/Release/WarriorsMapTool.exe'),'dependency-state',
                         str(ROOT/'WARRIORS.DIR'),str(ROOT/'WARRIORS.WAD'),str(ROOT/'SLUS_212.15'),'level51'],
                        capture_output=True,text=True,check=True)
level=json.loads(level_run.stdout)['activation_scenario'];roots={token,zlib.crc32(b'level51')}
level_candidates=[(i,r[0]) for i,r in enumerate(expected) if (1 if r[1] in roots else r[3])==1 and (1 if r[0] in roots else r[2])!=1]
assert [(r['record_ordinal'],r['key0']) for r in level['candidate_rows']]==level_candidates
assert level['next_record_ordinal']==(level_candidates[-1][0] if level_candidates and level_candidates[-1][1] else None)
assert level['events'][0]['token']==zlib.crc32(b'level51') and level['events'][0]['matched_table']
(out/'level51-activation.json').write_text(json.dumps(level,indent=2))
summary={'record_source_comparisons':count,'original_files_unchanged':len(originals),
         'object_records_source_compared':on,'unmatched_object_tokens':data['unmatched_object_tokens'],
         'ambiguous_object_tokens':data['ambiguous_object_tokens'],
         'startup_member':startup['member_index'],'startup_groups':group_count,
         'resolved_object_resources':resolved_count,'object_resource_dispatch_counts':resource_types,
         'level51_explicit_activation_candidates':len(level_candidates),
         'initializer_candidates_in_startup_container':sum(bool(c['startup_group_ordinals']) for c in candidates),
         'initializer_candidate_rows':len(candidates),'initializer_unique_candidate_keys':len({c['key0'] for c in candidates}),
         'unique_key0':data['unique_key0'],'unique_key1':data['unique_key1'],'opaque_tail_bytes':data['opaque_tail']['size'],
         'source_validation':'PASS','runtime_reference':'NOT_RUN','category_complete':False}
(out/'validation.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
