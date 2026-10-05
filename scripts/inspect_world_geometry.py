"""Extend bounded native packet inspection to newly loaded world-part geometry."""
import json
from collections import Counter
from load_map import SceneLoader,ROOT,Unsupported
from native_geometry_probe import words,take,native_probe

def children(data,lo,hi):
    result=[]
    while lo<hi:
        kind,length,stamp=words(take(data,0,hi),lo,3)
        if stamp!=0x1c02000a:raise Unsupported('unsupported geometry chunk stamp')
        take(take(data,0,hi),lo+12,length)
        result.append((kind,lo,length));lo+=12+length
    return result

def inspect_geometry(data,base):
    kind,length,stamp=words(data,0,3)
    if kind!=15 or length+12!=len(data) or stamp!=0x1c02000a:raise Unsupported('geometry root mismatch')
    entries=children(data,12,len(data))
    structs=[c for c in entries if c[0]==1]
    extensions=[c for c in entries if c[0]==3]
    if len(structs)!=1 or len(extensions)!=1:raise Unsupported('geometry struct/extension ambiguity')
    _,offset,size=structs[0]
    if size<16:raise Unsupported('short geometry struct')
    raw=words(data,offset+12,4)
    _,offset,size=extensions[0]
    plugins=children(data,offset+12,offset+12+size)
    bins=[c for c in plugins if c[0]==0x50e];natives=[c for c in plugins if c[0]==0x510]
    if len(bins)!=1 or len(natives)!=1:raise Unsupported('native/binmesh plugin ambiguity')
    _,bo,bl=bins[0]
    if bl<12:raise Unsupported('short binmesh')
    flags,mesh_count,index_count=words(data,bo+12,3)
    _,no,nl=natives[0]
    parsed=native_probe(take(data,no+12,nl),base+no+12,mesh_count,False)
    meshes=[]
    for mesh in parsed['meshes']:
        r={k:v for k,v in mesh.items() if k!='packets'}
        if 'packets' in mesh:
            packets=mesh['packets']
            r.update(tag_count=len(packets['tags']),tag_ids=dict(Counter(t['id'] for t in packets['tags'])),
                recognized_unpack_commands=len(packets['unpack_inputs']),
                unpack_formats=dict(Counter(s['format'] for s in packets['unpack_inputs'])),
                unclassified_ranges=packets['unclassified_ranges'])
        meshes.append(r)
    return {'geometry_struct_source':{'offset':base+structs[0][1]+12,'size':structs[0][2]},
        'geometry_struct_words':raw,'binmesh_flags':flags,'binmesh_index_count':index_count,
        'mesh_count':mesh_count,'native_source':parsed['source'],
        'declared_inner_length':parsed['declared_inner_length'],'actual_inner_bytes':parsed['actual_inner_bytes'],
        'writer_formula_matches':parsed['writer_formula_matches'],'meshes':meshes,
        'geometry_decoded':False,'scope':'Native payload and tag-storage interpretation; not VU output or triangle decoding'}

def main():
    loader=SceneLoader();results=[]
    try:
        for index,part in loader.parts.items():
            for atomic in part['atomics']:
                source=atomic['geometry'];loader.wad.seek(source['offset']);data=loader.wad.read(source['size'])
                item={'part_member':index,'sector_slot':atomic['sector_slot'],'source':source}
                try:item['inspection']=inspect_geometry(data,source['offset'])
                except Unsupported as exc:item.update(status='UNSUPPORTED',reason=str(exc))
                results.append(item)
        good=[x['inspection'] for x in results if 'inspection' in x]
        meshes=[m for x in good for m in x['meshes']]
        report={'parser':'inspect_world_geometry/0.1','source_identities':loader.identities,
            'atomic_geometries':len(results),'bounded_native_geometries':len(good),'native_mesh_bodies':len(meshes),
            'writer_formula_matches':sum(x['writer_formula_matches'] for x in good),
            'bounded_tag_walks':sum('tag_count' in m for m in meshes),
            'recognized_unpack_commands':sum(m.get('recognized_unpack_commands',0) for m in meshes),
            'geometry_decoded':False,'items':results}
        path=ROOT/'research/reports/world-loading/geometry-coverage.json'
        with path.open('x',encoding='utf-8') as f:json.dump(report,f,separators=(',',':'))
        print(json.dumps({k:v for k,v in report.items() if k not in ('items','source_identities')}))
        print('Unsupported:',Counter(x.get('reason') for x in results if 'inspection' not in x))
    finally:loader.close()

if __name__=='__main__':main()
