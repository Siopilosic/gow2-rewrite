"""Prepare the native viewer's selectable library from verified world memberships."""
import argparse
import json
from pathlib import Path
import re
from export_map_mesh import export_level
from load_map import SceneLoader, ROOT, Unsupported


def main():
    p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);args=p.parse_args()
    if args.output.exists():raise Unsupported('library output must be new')
    stems=[f.name.split('.')[0] for f in (ROOT/'research/reports/world-loading').glob('*.audit.scene.json')]
    stems=sorted(set(s for s in stems if re.fullmatch(r'level\d+',s)),key=lambda s:int(s[5:]))
    args.output.mkdir(parents=True);loader=SceneLoader();items=[];failures=[]
    try:
        for stem in stems:
            try:
                r=export_level(loader,stem,args.output/stem)
                item={'level':stem,'triangles':r['triangle_count'],'parts':r['loaded_atomics'],
                      'skipped':len(r['skipped_atomics']),
                      'count_differences':sum(x['stored_visible_count_difference']!=0 for x in r['items']),
                      'worlds':r['worlds']}
                items.append(item);print(json.dumps(item),flush=True)
            except Unsupported as e:
                failures.append({'level':stem,'reason':str(e)});print(stem+': '+str(e),flush=True)
        with (args.output/'library.json').open('x') as f:json.dump({'maps':items,'unavailable':failures,'source_identities':loader.identities},f,indent=2)
        with (args.output/'catalog.tsv').open('x') as f:
            f.write('WMVCATALOG1\n')
            for i in items:f.write(f"{i['level']}\t{i['level']}/map.wmv\t{i['triangles']}\t{i['parts']}\t{i['skipped']}\t{i['count_differences']}\n")
    finally:loader.close()


if __name__=='__main__':main()
