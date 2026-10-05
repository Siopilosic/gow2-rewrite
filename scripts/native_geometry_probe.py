"""Build-specific, bounded native packet research; no vertices or triangles exported."""
import hashlib
import argparse
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
ELF_HASH = '1a4e1c3891c930146e39ce340ccb02918cfe917032aafcdbbbe6541f75402369'
VERSION = 'native_geometry_probe/0.1'

class Unsupported(ValueError):
    pass

def take(data, offset, size):
    if offset < 0 or size < 0 or offset > len(data) or size > len(data)-offset:
        raise Unsupported(f'out of bounds: offset={offset}, size={size}, available={len(data)}')
    return data[offset:offset+size]

def words(data, offset, count):
    return struct.unpack('<'+'I'*count, take(data, offset, count*4))

def span(offset, size):
    return {'offset': offset, 'size': size}

def dma_probe(data, base, detailed=True):
    """Scan serialized tag storage using the original relocation walk, not DMA execution.

    Only CNT, REF and terminal tags are supported; NEXT/CALL/REFS fail closed.
    REF addresses are serialized quadword offsets, as established by 0x475d78.
    """
    cursor, tags, streams, covered = 0, [], [], []
    for _ in range(4096):
        w, address, vif0, vif1 = words(data, cursor, 4)
        kind, qwc = (w >> 28) & 7, w & 65535
        record = {'source': span(base+cursor,16), 'id':kind, 'qwc':qwc,
                  'address_word':address, 'vif_words':[vif0,vif1]}
        tags.append(record)
        covered.append((cursor,cursor+16))
        if kind == 3:
            target, size = address*16, qwc*16
            payload = take(data,target,size)
            record['referenced_data'] = span(base+target,size)
            covered.append((target,target+size))
            cmd = (vif1 >> 24) & 127
            # Restricted evidence profile: STCYCL CL=4 WL=1, one unmasked UNPACK.
            if vif0 == 0x01000104 and cmd in (0x6d,0x65,0x6e):
                components = ((cmd >> 2)&3)+1
                bits = 32 >> (cmd&3)
                count = (vif1 >> 16)&255 or 256
                used = count*components*bits//8
                take(payload,0,used)
                unsigned = bool(vif1 & 0x4000)
                fmt = {8:('B' if unsigned else 'b'),16:('H' if unsigned else 'h')}[bits]
                vals = struct.unpack('<'+fmt*(count*components),payload[:used])
                stream={'command_source':span(base+cursor+12,4),
                    'source':span(base+target,used), 'format':f'V{components}_{bits}',
                    'unsigned':unsigned, 'count':count, 'tops_relative':bool(vif1&0x8000),
                    'vu_address_field':vif1&1023, 'cycle_cl':4,'cycle_wl':1,
                    'vector_stride_bytes':components*bits//8,
                    'semantic':'UNKNOWN; integer unpack input, not world coordinates',
                    'padding':span(base+target+used,size-used)}
                if detailed:
                    stream['raw_vectors']=[list(vals[i:i+components]) for i in range(0,len(vals),components)]
                    stream['vector_sources']=[span(base+target+i*components*bits//8,components*bits//8) for i in range(count)]
                streams.append(stream)
            cursor += 16
        elif kind == 1:
            size=qwc*16
            take(data,cursor+16,size)
            record['inline_data']=span(base+cursor+16,size)
            covered.append((cursor+16,cursor+16+size))
            cursor += 16+size
        elif kind in (0,6,7):
            if kind in (6,7):
                take(data,cursor+16,qwc*16)
                record['inline_data']=span(base+cursor+16,qwc*16)
                covered.append((cursor+16,cursor+16+qwc*16))
            else:
                take(data,address*16,qwc*16)
                record['referenced_data']=span(base+address*16,qwc*16)
                covered.append((address*16,address*16+qwc*16))
            break
        else:
            raise Unsupported(f'unsupported serialized tag id {kind} at {cursor}')
    else:
        raise Unsupported('tag budget exceeded')
    merged=[]
    for lo,hi in sorted(covered):
        if merged and lo <= merged[-1][1]:
            merged[-1][1]=max(merged[-1][1],hi)
        else:
            merged.append([lo,hi])
    unknown=[]
    end=0
    for lo,hi in merged:
        if lo>end: unknown.append(span(base+end,lo-end))
        end=hi
    if end<len(data): unknown.append(span(base+end,len(data)-end))
    return {'tags':tags,'unpack_inputs':streams,'unclassified_ranges':unknown,
            'dma_execution':'NOT_EXECUTED; terminal RET requires caller context',
            'vu_execution':'NOT_EXECUTED'}

def native_probe(payload, base, mesh_count, detailed=True):
    if mesh_count < 1 or mesh_count > 4096:
        raise Unsupported('unsupported mesh count')
    kind, declared, stamp, platform = words(payload,0,4)
    if (kind,stamp,platform)!=(1,0x1c02000a,4):
        raise Unsupported('unsupported native header/platform')
    cursor, meshes = 16, []
    for ordinal in range(mesh_count):
        size, alignment = words(payload,cursor,2)
        body=take(payload,cursor+8,size)
        item={'ordinal':ordinal, 'size_field':span(base+cursor,4),
              'alignment_word':alignment,'source':span(base+cursor+8,size)}
        try:
            item['packets']=dma_probe(body,base+cursor+8,detailed)
        except Unsupported as exc:
            item['packet_status']='UNSUPPORTED'
            item['reason']=str(exc)
        meshes.append(item)
        cursor += 8+size
    if cursor != len(payload):
        raise Unsupported(f'unconsumed outer native payload: {len(payload)-cursor}')
    writer_declared=4+sum(0x1dc+m['source']['size'] for m in meshes)
    return {'source':span(base,len(payload)), 'declared_inner_length':declared,
            'actual_inner_bytes':len(payload)-12, 'reader_consumed_bytes':cursor,
            'writer_formula_length':writer_declared,'writer_formula_matches':declared==writer_declared,
            'length_policy':'Build-specific reader uses per-mesh size; strict outer bounds retained',
            'meshes':meshes,'confidence':'HIGH_CONFIDENCE_STATIC; no original execution',
            'geometry_decoded':False}

def hash_file(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()

def main():
    args=argparse.ArgumentParser(description=__doc__)
    args.add_argument('--output',type=Path,default=ROOT/'research/reports/native-geometry-step-verified')
    out=args.parse_args().output.resolve()
    if out.exists():
        raise SystemExit('Output directory already exists; preserve previous evidence.')
    catalogue=json.loads((ROOT/'research/reports/archive-analysis-verified/wad-members.json').read_text())
    expected=json.loads((ROOT/'research/reports/archive-analysis-verified/verification.json').read_text())
    identities={r['path']:r['sha256'] for r in expected['original_hashes']}
    for name in ('SLUS_212.15','WARRIORS.DIR','WARRIORS.WAD'):
        if hash_file(ROOT/name)!=identities[name]: raise SystemExit('Source identity mismatch: '+name)
    if identities['SLUS_212.15']!=ELF_HASH: raise SystemExit('Unsupported executable')
    results=[]
    with (ROOT/'WARRIORS.WAD').open('rb') as wad:
        for member in catalogue['items']:
            chunks=member['chunks']['candidate_chunks']
            for chunk in chunks:
                if chunk['type_word']!=0x510: continue
                start=chunk['source']['offset']
                member_range=member['candidate_wad_range']
                if not (member_range['offset']<=start and start+12+chunk['payload_size']<=member_range['offset']+member_range['size']):
                    raise Unsupported('catalogue native chunk outside member')
                wad.seek(start)
                if words(wad.read(12),0,3)!=(0x510,chunk['payload_size'],0x1c02000a):
                    raise Unsupported('native catalogue header differs from source')
                # Associate only siblings within the smallest enclosing extension.
                parents=[c for c in chunks if c['type_word']==3 and c['source']['offset']<start
                         and start+chunk['source']['size']<=c['source']['offset']+c['source']['size']]
                if not parents: continue
                parent=min(parents,key=lambda c:c['source']['size'])
                bins=[c for c in chunks if c['type_word']==0x50e and c['depth']==chunk['depth']
                      and parent['source']['offset']<c['source']['offset']<start]
                if len(bins)!=1: continue
                wad.seek(bins[0]['source']['offset']+12)
                header=wad.read(12)
                if len(header)!=12: raise Unsupported('truncated bin mesh')
                flags,count,indices=words(header,0,3)
                wad.seek(start+12)
                payload=wad.read(chunk['payload_size'])
                item={'index':member['index'],'native_header':span(start,12),
                      'bin_mesh':{'source':bins[0]['source'],'flags':flags,'mesh_count':count,'index_count':indices}}
                try: item['native']=native_probe(payload,start+12,count,member['index']==201)
                except Unsupported as exc: item.update(status='UNSUPPORTED',reason=str(exc))
                results.append(item)
    out.mkdir()
    evidence=[]
    for label,start,end in [('plugin-registration',0x472a58,0x472ad8),
            ('geometry-reader',0x46b438,0x46be80),('geometry-list-reader',0x468b00,0x468da0),
            ('native-wrapper',0x4739d8,0x473a48),('native-reader',0x475b20,0x475e00),
            ('native-writer',0x475898,0x475b20),('chunk-find',0x47bab8,0x47bc98),
            ('binmesh-reader',0x46d9d8,0x46e0c8)]:
        text=subprocess.check_output([sys.executable,str(ROOT/'research/scripts/loader_probe.py'),hex(start),hex(end)],text=True)
        (out/(label+'.asm.txt')).write_text(text)
        evidence.append({'label':label,'file':label+'.asm.txt','source_file':'SLUS_212.15',
                         'sha256':ELF_HASH,'offset':start-0xff000,'size':end-start,'virtual_address':start})
    good=[r for r in results if 'native' in r]
    report={'parser':VERSION,'source_identities':{k:identities[k] for k in ('SLUS_212.15','WARRIORS.DIR','WARRIORS.WAD')},
            'scope':'Previously bounded native chunks with one preceding binmesh sibling; not whole-corpus geometry recognition',
            'evidence':evidence,'candidates':len(results),'bounded_native_payloads':len(good),
            'writer_formula_matches':sum(r['native']['writer_formula_matches'] for r in good),
            'items':results,'geometry_decoded':False}
    (out/'native-packets.json').write_text(json.dumps(report,indent=2))
    selected=[r for r in results if r['index']==201]
    (out/'entry-201.json').write_text(json.dumps({**{k:v for k,v in report.items() if k!='items'},'items':selected},indent=2))
    print(json.dumps({k:v for k,v in report.items() if k not in ('items','evidence','source_identities')},indent=2))

if __name__=='__main__': main()
