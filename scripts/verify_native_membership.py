"""Compare the C++ category-1 import against the independent Python world loader."""
import json
from pathlib import Path
import subprocess
from load_map import SceneLoader

root=Path(__file__).resolve().parents[2]
out=root/'research/reports/membership-native';out.mkdir(exist_ok=True)
stems=sorted({w for f in (root/'research/reports/map-library').glob('*/mesh.json') for w in json.loads(f.read_text())['worlds']})
assert len(stems)==136
(out/'worlds.txt').write_text('\n'.join(stems)+'\n')
exe=root/'research/native/build/Release/WarriorsMapTool.exe'
cmd=[str(exe),'membership-batch',str(root/'WARRIORS.DIR'),str(root/'WARRIORS.WAD'),str(root/'SLUS_212.15'),str(out/'worlds.txt')]
run=subprocess.run(cmd,capture_output=True,text=True,check=True)
native=json.loads(run.stdout);assert len(native)==len(stems)
loader=SceneLoader();slot_count=atomic_count=0
try:
    for stem,document in zip(stems,native):
        py=loader.load_world(stem)
        assert document['nodes'][0]['member_index']==py['source']['member_index']
        assert {x['member_index'] for x in document['nodes'][1:]}=={p['source']['member_index'] for p in py['parts']}
        assert [(x['slot'],x['group'],x['active']) for x in document['world_slots']]==[(x['slot'],x['group'],x['active']) for x in py['layout']['slots']]
        assert document['atomic_count']==len(py['instances'])
        for edge in document['edges']:
            index=int(edge['to'].rsplit(':',1)[1]);part=loader.parts[index]
            assert [(x['sector_slot'],x['geometry'],x['extension']) for x in edge['atomic_records']]==[(x['sector_slot'],x['geometry'],x['extension']) for x in part['atomics']]
        for slot in document['world_slots']:
            span=slot['source'];loader.wad.seek(span['offset'])
            assert bytes.fromhex(slot['raw_hex'])==loader.wad.read(span['size'])
            assert slot['opaque_placement']['offset']==span['offset']+20
            slot_count+=1
        for node in document['nodes']:
            span=node['dir_record_source']
            assert bytes.fromhex(node['dir_record_hex'])==loader.directory[span['offset']:span['offset']+span['size']]
        assert document['structural_membership_validated'] and not document['complete_dependency_closure'] and not document['category_complete']
        atomic_count+=document['atomic_count']
finally:loader.close()
(out/'membership.json').write_text(run.stdout)
summary={'native_worlds_compared':len(native),'atomic_memberships_compared':atomic_count,'slot_hex_source_checks':slot_count,
         'independent_python_comparison':'PASS','source_identity_checks':'PASS',
         'category_complete':False,'remaining':'Global, conditional and non-world resource dependency closure'}
(out/'validation.json').write_text(json.dumps(summary,indent=2))
print(json.dumps(summary))
