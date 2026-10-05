"""Audit observed level stems and independently verify native part boundaries."""
import json
import re
import struct
import subprocess
import sys
from load_map import SceneLoader,ROOT,Unsupported

def main():
    out=ROOT/'research/reports/world-loading'
    loader=SceneLoader()
    try:
        elf=(ROOT/'SLUS_212.15').read_bytes()
        stems={m.group().decode().lower():{'source_file':'SLUS_212.15','offset':m.start(),'size':len(m.group())}
               for m in re.finditer(rb'level[0-9]+',elf,re.I)}
        samples=json.loads((ROOT/'research/reports/archive-analysis-verified/strings.json').read_text())['items']
        for member in samples:
            for sample in member['strings']['samples']:
                for m in re.finditer(r'level[0-9]+',sample['text'],re.I):
                    stems.setdefault(m.group().lower(),{**sample['source'],'offset':sample['source']['offset']+m.start(),'size':len(m.group())})
        checks=0
        for index,part in loader.parts.items():
            o,n,_=loader.rows[index];loader.wad.seek(o);data=loader.wad.read(n)
            p=28+struct.unpack_from('<I',data,20)[0]
            count=struct.unpack_from('<I',data,p)[0];p+=4
            assert count==len(part['atomics']);checks+=1
            for atomic in part['atomics']:
                slot,t,length,stamp=struct.unpack_from('<4I',data,p)
                assert (slot,t,stamp)==(atomic['sector_slot'],20,0x1c02000a)
                assert atomic['source']['offset']==o+p+4 and atomic['source']['size']==length+12
                p+=16+length;assert p<=len(data);checks+=3
            assert p==len(data);checks+=1
        scenes=[];unresolved=[];loaded_indices=set()
        for stem,source in sorted(stems.items()):
            try: scene=loader.load_level(stem)
            except Unsupported as exc:
                unresolved.append({'level_stem':stem,'reason':str(exc)});continue
            for w in scene['worlds']:
                loaded_indices.add(w['source']['member_index'])
                for instance in w['instances']:
                    loc=instance['translation_source'];loader.wad.seek(loc['offset'])
                    raw=loader.wad.read(12)
                    assert list(struct.unpack('<3I',raw))==instance['translation_bits']
                    assert list(struct.unpack('<3f',raw))==instance['translation'];checks+=2
            scene['observed_stem_source']={**source,'sha256':loader.identities.get(source['source_file'],'see source report')}
            with (out/(stem+'.audit.scene.json')).open('x',encoding='utf-8') as f:json.dump(scene,f,separators=(',',':'),allow_nan=False)
            scenes.append({'level':stem,'worlds':len(scene['worlds']),'parts':sum(len(w['parts']) for w in scene['worlds']),
                'placements':sum(len(w['instances']) for w in scene['worlds']),
                'structural_loading_complete':scene['structural_loading_complete'],'unresolved_worlds':scene['unresolved_worlds']})
        evidence=[]
        for label,start,end in [('world-reader',0x410a50,0x410b68),('part-reader',0x411234,0x411444),
                ('sector-plugin',0x198e20,0x198ff0),('sector-fields',0x199008,0x1990c0),
                ('slot-registration',0x411010,0x4110c0),('world-names',0x40dd88,0x40df10),
                ('part-name-template',0x410980,0x4109c8)]:
            text=subprocess.check_output([sys.executable,str(ROOT/'research/scripts/loader_probe.py'),hex(start),hex(end)],text=True)
            (out/(label+'.asm.txt')).write_text(text)
            evidence.append({'file':label+'.asm.txt','source_file':'SLUS_212.15','sha256':loader.identities['SLUS_212.15'],
                             'offset':start-0xff000,'size':end-start,'virtual_address':start})
        summary={'parser':'audit_world_loading/0.1','source_identities':loader.identities,
                 'loaded_parts':len(loader.parts),'atomic_count':sum(p['atomic_count'] for p in loader.parts.values()),
                 'independent_checks':checks,'observed_level_stems':len(stems),'scenes':scenes,
                 'unresolved_stems':unresolved,'unique_loaded_worlds':len(loaded_indices),
                 'evidence':evidence,'one_to_one_verified':False,'render_ready':False}
        with (out/'verification.json').open('x',encoding='utf-8') as f:json.dump(summary,f,indent=2)
        print(json.dumps({k:v for k,v in summary.items() if k not in ('source_identities','scenes','evidence','unresolved_stems')}))
        print('Incomplete scenes:',[s for s in scenes if not s['structural_loading_complete']])
    finally:loader.close()

if __name__=='__main__':main()
