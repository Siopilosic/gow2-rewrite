"""Test limited flag/mask hypotheses without discarding original index words."""
import collections
import hashlib
import json
from pathlib import Path
import struct
import sys

root=Path(__file__).resolve().parents[2]
out=Path(sys.argv[1]).resolve()
b=(root/'WARRIORS.DIR').read_bytes()
rows=list(struct.iter_unpack('<III',b[16:]))
wad_size=(root/'WARRIORS.WAD').stat().st_size
tests=[]
for ow,lw in [(0,1),(2,1),(0,2)]:
    for mask in [0xffffffff,0x7fffffff,0x0fffffff,0x00ffffff,0x0000ffff]:
        valid=ordered=changed=0;previous_end=0
        for i,r in enumerate(rows):
            o,n=r[ow]&mask,r[lw]&mask
            valid+=o+n<=wad_size
            ordered+=i==0 or o>=previous_end
            changed+=(o,n)!=(r[ow],r[lw])
            previous_end=o+n
        tests.append({'offset_word':ow,'length_word':lw,'mask':hex(mask),'valid_ranges':valid,'ordered_nonoverlap_rows':ordered,'changed_rows':changed,'interpretation':'unchanged representation; cannot test proposed flags' if changed==0 else 'changes source interpretation; needs loader evidence'})
stats=[{'word':i,'distinct_values':len(set(r[i] for r in rows)),'bitwise_or':hex(__import__('functools').reduce(int.__or__,(r[i] for r in rows))),'upper_byte_histogram':dict(collections.Counter(str(r[i]>>24) for r in rows))} for i in range(3)]
result={'parser':'packed_field_probe/0.1','source_file':'WARRIORS.DIR','sha256':hashlib.sha256(b).hexdigest(),'offset':16,'size':len(b)-16,'confidence':'CONFIRMED','scope':'Arithmetic and bit-distribution observations only','field_statistics':stats,'mask_models':tests,'limitations':['Finite mask set does not exclude arbitrary packed formats','Zero observed flag bits cannot disprove a flags field','Executable uses unmasked record words at inspected consumers; other consumers not exhaustively analysed']}
(out/'packed-field-hypotheses.json').write_text(json.dumps(result,indent=2))
print('Tested',len(tests),'mask hypotheses; retained raw fields and inconclusive cases.')
