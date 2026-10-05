"""Load source-mapped world/sector scene descriptions. No guessed render geometry."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re
import struct
import zlib
from native_geometry_probe import take, words, Unsupported, ROOT, ELF_HASH

PARSER='WarriorsSceneLoader/0.1'

def world_layout(data,base=0):
    """WorldPS2 0x410a50 plus registered sector plugin 0x3f1."""
    groups=words(data,0,1)[0]
    if groups>4096: raise Unsupported('world group budget')
    def chunk(at,end,expected=None):
        t,n,s=words(take(data,0,end),at,3)
        if s!=0x1c02000a or (expected is not None and t!=expected):
            raise Unsupported('unexpected world chunk header')
        take(take(data,0,end),at+12,n)
        return t,n,at+12+n
    _,txd_length,p=chunk(4,len(data),0x16)
    world_offset=p
    _,world_length,end=chunk(p,len(data),0xb)
    if end!=len(data): raise Unsupported('world member trailing bytes')
    slots=[]; opaque=[]; nodes=0
    def walk(lo,hi,depth=0,parent=None):
        nonlocal nodes
        if depth>128: raise Unsupported('BSP depth budget')
        while lo<hi:
            t,n,nxt=chunk(lo,hi); nodes+=1
            if nodes>100000: raise Unsupported('BSP node budget')
            if t==0x3f1:
                if n!=20 or parent!=3: raise Unsupported('sector plugin size/context')
                slot,group,*bits=words(data,lo+12,5)
                xyz=struct.unpack('<3f',take(data,lo+20,12))
                if any(not (-float('inf')<v<float('inf')) for v in xyz):
                    raise Unsupported('non-finite sector translation')
                slots.append({'slot':slot,'group':group,'translation':list(xyz),
                    'translation_bits':bits,'source':{'offset':base+lo,'size':32},
                    'translation_source':{'offset':base+lo+20,'size':12},
                    'active':slot!=0xffffffff})
            elif t in (0xb,9,10,3): walk(lo+12,nxt,depth+1,t)
            else: opaque.append({'type':t,'offset':base+lo,'size':12+n})
            lo=nxt
    walk(world_offset+12,end,0,0xb)
    active=[s for s in slots if s['active']]
    ids=[s['slot'] for s in active]
    if len(ids)!=len(set(ids)): raise Unsupported('duplicate active world slot')
    if any(s['group']<1 or s['group']>groups for s in active):
        raise Unsupported('sector group outside declared world groups')
    return {'declared_groups':groups,'texture_dictionary':{'offset':base+4,'size':txd_length+12},
        'world_chunk':{'offset':base+world_offset,'size':world_length+12},
        'slots':slots,'opaque_chunks':opaque,'chunk_count':nodes}

class SceneLoader:
    def __init__(self,root=ROOT):
        self.root=Path(root)
        self.catalogue=json.loads((self.root/'research/reports/world-loading/world-parts.json').read_text())
        self.parts={p['member_index']:p for p in self.catalogue['parts']}
        self.directory=(self.root/'WARRIORS.DIR').read_bytes()
        self.rows=list(struct.iter_unpack('<III',self.directory[16:]))
        self.keys=defaultdict(list)
        for i,(_,_,key) in enumerate(self.rows): self.keys[key].append(i)
        self.wad=(self.root/'WARRIORS.WAD').open('rb')
        self.identities={}
        expected=json.loads((self.root/'research/reports/archive-analysis-verified/verification.json').read_text())
        originals={r['path']:r['sha256'] for r in expected['original_hashes']}
        for name in ('WARRIORS.DIR','WARRIORS.WAD','SLUS_212.15'):
            with (self.root/name).open('rb') as f: self.identities[name]=hashlib.file_digest(f,'sha256').hexdigest()
            if self.identities[name]!=originals[name]: raise Unsupported('source identity changed: '+name)
        if self.identities['SLUS_212.15']!=ELF_HASH: raise Unsupported('unsupported executable build')
        for p in self.parts.values():
            i=p['member_index']; o,n,_=self.rows[i]
            if p['source']['sha256']!=self.identities['WARRIORS.WAD'] or (p['source']['offset'],p['source']['size'])!=(o,n):
                raise Unsupported('part catalogue identity mismatch')

    def close(self): self.wad.close()

    def lookup(self,name):
        matches=self.keys[zlib.crc32(name.lower().encode('ascii'))]
        if len(matches)!=1: raise Unsupported(f'lookup has {len(matches)} matches: {name}')
        return matches[0]

    def source(self,index):
        o,n,_=self.rows[index]
        return {'source_file':'WARRIORS.WAD','sha256':self.identities['WARRIORS.WAD'],'offset':o,'size':n,'member_index':index}

    def load_world(self,stem):
        name='./ee_files/'+stem+'_sec.wld'
        index=self.lookup(name);o,n,_=self.rows[index]
        self.wad.seek(o);layout=world_layout(self.wad.read(n),o)
        active=[s for s in layout['slots'] if s['active']]
        by_group=defaultdict(dict)
        for slot in active: by_group[slot['group']][slot['slot']]=slot
        loaded=[];instances=[]
        for group in range(1,layout['declared_groups']+1):
            part_name='./ee_files/'+stem+'_ms'+str(group)+'.sec'
            part_index=self.lookup(part_name)
            if part_index not in self.parts: raise Unsupported('referenced member has no supported part layout')
            part=self.parts[part_index]
            wanted=set(by_group[group]);actual=[a['sector_slot'] for a in part['atomics']]
            if len(actual)!=len(set(actual)) or set(actual)!=wanted:
                raise Unsupported(f'world/part slot disagreement in {part_name}: expected {sorted(wanted)}, actual {actual}')
            loaded.append({'group':group,'lookup_name':part_name,'source':self.source(part_index),
                           'texture_dictionary':part['texture_dictionary'],'atomic_count':part['atomic_count']})
            for atomic in part['atomics']:
                slot=by_group[group][atomic['sector_slot']]
                # Byte-for-byte stored translation; no axis conversion or normalization.
                instances.append({'slot':slot['slot'],'group':group,'part_member':part_index,
                    'atomic':atomic['source'],'geometry':atomic['geometry'],
                    'translation':slot['translation'],'translation_bits':slot['translation_bits'],
                    'translation_source':{**slot['translation_source'],'source_file':'WARRIORS.WAD','sha256':self.identities['WARRIORS.WAD']},
                    'render_ready':False})
        return {'world_stem':stem,'lookup_name':name,'source':self.source(index),'layout':layout,
                'parts':loaded,'instances':instances,'membership_status':'CRC_TEMPLATE_AND_EXACT_SLOT_GROUP_AGREEMENT',
                'interior_exterior':'NOT_CLASSIFIED','render_ready':False}

    def load_level(self,stem):
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,64}',stem): raise Unsupported('invalid level stem')
        layers=[];missing=[]
        for suffix in ('s','d',''):
            candidate=stem+suffix
            name='./ee_files/'+candidate+'_sec.wld'
            if not self.keys.get(zlib.crc32(name.encode('ascii').lower())): continue
            try: layers.append(self.load_world(candidate))
            except Unsupported as exc: missing.append({'world_stem':candidate,'reason':str(exc)})
        if not layers and not missing: raise Unsupported('no world variants resolve for '+stem)
        return {'parser':PARSER,'level_stem':stem,'source_identities':self.identities,'worlds':layers,
                'unresolved_worlds':missing,'structural_loading_complete':bool(layers) and not missing,
                'one_to_one_verified':False,'render_ready':False,
                'limitations':['Geometry/VU decoding is incomplete','Texture pixels and materials not decoded',
                    'Interior/exterior labels not established','Original runtime comparison not performed',
                    'CRC-derived names corroborated by layout/slot agreement, not collision-proof filenames']}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('level',help='Observed level stem, e.g. level100; loads all resolving s/d/plain variants')
    p.add_argument('--output',type=Path,required=True,help='New scene JSON file')
    args=p.parse_args()
    loader=SceneLoader()
    try: scene=loader.load_level(args.level)
    finally: loader.close()
    with args.output.open('x',encoding='utf-8') as f:json.dump(scene,f,indent=2,allow_nan=False)
    print(json.dumps({'level':args.level,'worlds':len(scene['worlds']),
        'parts':sum(len(w['parts']) for w in scene['worlds']),
        'instances':sum(len(w['instances']) for w in scene['worlds']),
        'unresolved_worlds':scene['unresolved_worlds'],'render_ready':False}))
    if not scene['structural_loading_complete']:raise SystemExit(1)

if __name__=='__main__':main()
