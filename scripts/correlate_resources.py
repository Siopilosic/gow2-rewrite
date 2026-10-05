"""Build-specific static evidence and resource catalogue enrichment; no writes to inputs."""
from pathlib import Path
import collections
import hashlib
import json
import re
import struct
import zlib
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
out = Path(sys.argv[1]).resolve() if len(sys.argv)>1 else root / 'research/reports/archive-analysis'
elf = (root/'SLUS_212.15').read_bytes()
elf_hash = hashlib.sha256(elf).hexdigest()
assert elf_hash == '1a4e1c3891c930146e39ce340ccb02918cfe917032aafcdbbbe6541f75402369', 'Build-specific evidence addresses require original build'
manifest = json.loads((root/'research/reports/initial/manifest.json').read_text())
hashes = {f['path']:f['sha256'] for f in manifest['files']}
summary=json.loads((out/'summary.json').read_text())
assert summary['dir']['sha256']==hashes['WARRIORS.DIR'] and summary['wad']['sha256']==hashes['WARRIORS.WAD'], 'Native scan must match the original source snapshot'
directory = (root/'WARRIORS.DIR').read_bytes()
assert hashlib.sha256(directory).hexdigest() == hashes['WARRIORS.DIR']
rows = list(struct.iter_unpack('<III',directory[16:]))
def save(name, data):
    (out/name).write_text(json.dumps(data,indent=2),encoding='utf-8')
def loc(path,offset,size):
    return {'source_file':path,'sha256':hashes[path],'offset':offset,'size':size}
def envelope(items):
    return {'parser':'correlate_resources/0.1','elf_sha256':elf_hash,'wad_sha256':hashes['WARRIORS.WAD'],'confidence':'HIGH_CONFIDENCE','items':items}

# Base and stride are read at VA 0x144524 and 0x14454c/50/54, not selected by label order.
table=[]
for index in range(84):
    off=0x40c2e0+index*12
    ptr,after,reader=struct.unpack_from('<III',elf,off)
    nameoff=ptr-0xff000
    assert 0<=nameoff<len(elf)
    name=elf[nameoff:].split(b'\0',1)[0].decode('ascii')
    table.append({'type_id':index,'original_label':name,'source':loc('SLUS_212.15',off,12),
                  'label_source':loc('SLUS_212.15',nameoff,len(name)),
                  'post_read_callback_va':after or None,'stream_reader_va':reader or None,
                  'evidence':['0x00144524: table base','0x00144548..0x00144564: type*12, callback loads'],
                  'confidence':'CONFIRMED','scope':'Exact build table labels and dispatch fields; asset semantics still require decoding'})
save('chunk-dispatch.json',envelope(table))

# A linear resource hypothesis: 16-byte count header, then count*(16-byte header + size).
# Reject incomplete interpretations instead of forcing a semantic classification.
catalogue=[]; shape_counts=collections.Counter(); types=collections.Counter()
with (root/'WARRIORS.WAD').open('rb') as f:
    for index,(offset,size,key) in enumerate(rows):
        f.seek(offset); head=f.read(min(size,16))
        item={'index':index,'source':loc('WARRIORS.WAD',offset,size),'index_source':loc('WARRIORS.DIR',16+index*12,12),'raw_key':key,'blocks':[],'status':'NOT_RECOGNISED'}
        if len(head)==16:
            h=struct.unpack('<4I',head); item['raw_header_words']=h
            if 0<h[0]<=4096:
                cursor=16; total=0; valid=True
                for j in range(h[0]):
                    if cursor+16>size:valid=False;break
                    f.seek(offset+cursor); raw=f.read(16); tag,length,u8,u12=struct.unpack('<4I',raw)
                    if length>size-cursor-16:valid=False;break
                    label=table[tag]['original_label'] if tag<len(table) else None
                    item['blocks'].append({'ordinal':j,'header':loc('WARRIORS.WAD',offset+cursor,16),'payload':loc('WARRIORS.WAD',offset+cursor+16,length),'type_word':tag,'length_word':length,'unknown_08':u8,'unknown_0c':u12,'dispatch_label_candidate':label,'confidence':'HIGH_CONFIDENCE' if label else 'UNKNOWN'})
                    total+=length;cursor+=16+length
                item['consumed_bytes']=cursor;item['unexplained_tail_bytes']=size-cursor
                item['status']='EXACT_LINEAR_RESOURCE_LAYOUT' if valid and cursor==size else 'PARTIAL_OR_REJECTED_LINEAR_LAYOUT'
                item['sum_payload_lengths_matches_header_word1']=total==h[1]
                if item['status']=='EXACT_LINEAR_RESOURCE_LAYOUT':
                    for block in item['blocks']:types[block['type_word']]+=1
        shape_counts[item['status']]+=1;catalogue.append(item)
save('resource-catalogue.json',envelope(catalogue))
save('resource-families.json',envelope({'layout_counts':shape_counts,'type_counts':{str(k):{'count':v,'original_label':table[k]['original_label'] if k<len(table) else None} for k,v in types.items()}}))

# CRC32 recovered from polynomial 0xedb88320, initial -1, final NOT; case table read from ELF.
ctype=elf[0x58c531-0xff000:0x58c531-0xff000+128]
def lower_original(s):
    return bytes((c+32)&255 if c<128 and ctype[c]&1 else c for c in s)
assert all(lower_original(bytes([i])) == bytes([i]).lower() for i in range(1,128))
keys={k:i for i,(_,_,k) in enumerate(rows)}
matches=[];seen=set()
for m in re.finditer(rb'[\x20-\x7e]{3,240}',elf):
    s=m.group()
    variants=[('literal',s)]
    if re.fullmatch(rb'[A-Za-z0-9_./\\-]{3,80}',s):
        variants += [('observed_ee_files_prefix',b'./ee_files/'+s)]
        if b'.' not in s and b'/' not in s and b'\\' not in s:
            variants += [('observed_msb_template',b'./ee_files/'+s+b'.msb'),('observed_msd_template',b'./ee_files/'+s+b'.msd')]
    for derivation,text in variants:
        normalized=lower_original(text);key=zlib.crc32(normalized)
        if key in keys and (key,normalized) not in seen:
            seen.add((key,normalized));matches.append({'member_index':keys[key],'word2':key,'candidate_lookup_name':normalized.decode('ascii'),'derivation':derivation,'name_string_source':loc('SLUS_212.15',m.start(),len(s)),'confidence':'HIGH_CONFIDENCE' if derivation=='literal' else 'PROBABLE','evidence':['0x00143ea0 polynomial/table','0x00143f68 CRC loop','0x00143fd8 case normalization','0x00149118 compares record+8'],'warning':'CRC match, not proof of unique original filename or map membership'})
save('name-candidates.json',envelope(matches))

# Independent bytewise vs table CRC implementations, plus standard library check.
def crc_bitwise(s):
    crc=0xffffffff
    for b in s:
        crc^=b
        for _ in range(8):crc=(crc>>1)^(0xedb88320 if crc&1 else 0)
    return crc^0xffffffff
for s in [b'',b'123456789',b'./ee_files/example.msb']+[x['candidate_lookup_name'].encode() for x in matches]:
    assert crc_bitwise(s)==zlib.crc32(s)
save('crc-validation.json',envelope({'polynomial':'0xedb88320','ascii_lowercase_table_checked':True,'test_vector_123456789':hex(crc_bitwise(b'123456789')),'matched_name_candidates':len(matches),'bitwise_vs_zlib_pass':True,'non_ascii_normalization':'not established'}))

# Save inspected instruction ranges verbatim with addresses and file offsets.
spans=[('dir-constructor',0x149160,0x149240),('name-lookup',0x1490b8,0x149160),('crc-table',0x143ea0,0x143f00),('crc-string',0x143f68,0x143fd0),('lowercase',0x143fd8,0x144050),('boot-archive-paths',0x40c5e0,0x40c628),('resource-dispatch',0x14451c,0x144688),('size-lookup',0x40c648,0x40c668),('range-consumer',0x14c6b4,0x14c714),('preinstance-reader',0x17f2c0,0x17f450),('map-init-reference',0x160000,0x1601a0)]
spans.append(('linear-resource-reader',0x144180,0x144348))
for name,start,end in spans:
    p=subprocess.run([sys.executable,str(root/'research/scripts/loader_probe.py'),hex(start),hex(end)],capture_output=True,text=True,check=True)
    (out/(name+'.asm.txt')).write_text(p.stdout,encoding='utf-8')
save('loader-evidence.json',envelope([{'id':name,'source':loc('SLUS_212.15',start-0xff000,end-start),'virtual_start':hex(start),'virtual_end':hex(end),'dump':name+'.asm.txt','confidence':'HIGH_CONFIDENCE','limitation':'Static analysis; no PS2 execution trace; unhandled EE/VU instructions retained opaque'} for name,start,end in spans]))
print('Layout counts',dict(shape_counts));print('Typed blocks',types.most_common(12));print('Name candidates',len(matches));print(matches[:8])
