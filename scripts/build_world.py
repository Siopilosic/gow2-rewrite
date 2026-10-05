"""Build and export unified world-space maps: decode, anchor, diagnose, write.

One command covers the whole path for a level or for every level in the library:

    python research/scripts/build_world.py --output research/reports/world-export-1 --level level51
    python research/scripts/build_world.py --output research/reports/world-export-1 --all-levels

Each level gets its own directory holding `scene.gltf` + `scene.bin` + `textures/`,
optionally `scene.obj` + `scene.mtl`, and `manifest.json` with per-instance byte
provenance, the material and texture tables, the export options in force and the
world-space diagnostics. `world-report.json` aggregates every level.

Full-map export is the default and the only claim made: if any referenced atomic
or referenced texture does not decode, that level is refused and listed in the
report rather than written partially. `--allow-partial` turns that off explicitly
and marks the affected manifests.
"""
import argparse
import json
from pathlib import Path
import re
import sys
import time

from native_geometry_probe import Unsupported, ROOT
from load_map import SceneLoader
import export_scene
import world_space

VERSION = 'WarriorsWorldBuild/0.1'


def library_stems():
    """The level stems the verified world-loading audit already resolved."""
    stems = [path.name.split('.')[0]
             for path in (ROOT / 'research/reports/world-loading').glob('*.audit.scene.json')]
    return sorted({s for s in stems if re.fullmatch(r'level\d+', s)},
                  key=lambda s: int(s[5:]))


def options_from(args):
    return {'axis': args.axis, 'unit_scale': args.unit_scale,
            'alpha_policy': args.alpha, 'alpha_mode': args.alpha_mode,
            'double_sided': not args.single_sided,
            'vertex_colours': not args.no_vertex_colours,
            'texture_chain': list(args.chain),
            'require_complete': not args.allow_partial,
            'policy_note': 'axis, unit_scale, alpha_mode, double_sided and '
                           'vertex_colours are caller choices with no verified '
                           'original counterpart; alpha_policy gs128 rescales GS '
                           'nominal 0..128 alpha for viewing only'}


def build_one(loader, stem, output, args, options):
    started = time.time()
    resolver = world_space.TextureResolver(loader, tuple(args.chain))
    scene = world_space.build_level(loader, stem, tuple(args.chain), resolver)
    diagnostics = world_space.analyse(scene, tolerance=args.adjacency_tolerance)
    if scene.unresolved_worlds and not args.allow_partial:
        raise Unsupported(
            'world variants did not resolve, full-map export refused: '
            + str(scene.unresolved_worlds))
    plan = export_scene.ExportPlan(scene, resolver, require_complete=not args.allow_partial)
    directory = output / stem
    written = {}
    if 'gltf' in args.formats:
        written['gltf'] = export_scene.export_gltf(scene, plan, directory, options)
    if 'obj' in args.formats:
        written['obj'] = export_scene.export_obj(scene, plan, directory, options)
    if 'wmv3' in args.formats:
        written['wmv3'] = export_scene.export_wmv3(scene, plan, directory, options)
    report = export_scene.manifest(scene, plan, options, diagnostics)
    report['written'] = written
    report['texture_failures'] = plan.texture_failures
    report['build_seconds'] = round(time.time() - started, 2)
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / 'manifest.json').open('w', encoding='utf-8') as handle:
        json.dump(report, handle, separators=(',', ':'), allow_nan=False)
    return {'level': stem, 'triangles': scene.triangle_count,
            'instances': len(scene.instances), 'materials': len(scene.materials),
            'textures': len(plan.textures), 'skipped': len(scene.skipped),
            'overlapping_pairs': diagnostics['overlapping_pairs'],
            'adjacent_pairs': diagnostics['adjacent_pairs'],
            'position_scale_consistent': diagnostics['position_scale_consistent'],
            'precedence_conflicts': len(diagnostics['texture_precedence_conflicts']),
            'unresolved_texture_names': diagnostics['unresolved_texture_names'],
            'vertex_colour_component_range': diagnostics['vertex_colour_component_range'],
            'sector_translation_axis_ratio': diagnostics['sector_translation_axis_ratio'],
            'bounds': diagnostics['bounds'], 'seconds': report['build_seconds'],
            'written': written}


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--output', type=Path, required=True, help='new directory')
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--level', help='one level stem, e.g. level51')
    group.add_argument('--all-levels', action='store_true')
    parser.add_argument('--formats', default='gltf',
                        type=lambda v: [x.strip() for x in v.split(',') if x.strip()],
                        help='comma separated: gltf,obj,wmv3 (wmv3 feeds the native viewer)')
    parser.add_argument('--axis', default='raw', choices=sorted(export_scene.AXIS_TRANSFORMS),
                        help='raw keeps the game frame; anything else is your assertion')
    parser.add_argument('--unit-scale', type=float, default=1.0)
    parser.add_argument('--alpha', default='gs128', choices=('gs128', 'raw'))
    parser.add_argument('--alpha-mode', default='OPAQUE',
                        choices=('OPAQUE', 'MASK', 'BLEND'))
    parser.add_argument('--single-sided', action='store_true')
    parser.add_argument('--no-vertex-colours', action='store_true')
    parser.add_argument('--chain', default='part,world',
                        type=lambda v: [x.strip() for x in v.split(',') if x.strip()],
                        help='texture dictionary precedence order')
    parser.add_argument('--adjacency-tolerance', type=float, default=0.0)
    parser.add_argument('--allow-partial', action='store_true',
                        help='write a level even when atomics or textures are missing')
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit('Output directory already exists; choose a new one.')
    for tier in args.chain:
        if tier not in ('part', 'world'):
            raise SystemExit('Unknown dictionary tier: ' + tier)
    for kind in args.formats:
        if kind not in ('gltf', 'obj', 'wmv3'):
            raise SystemExit('Unknown format: ' + kind)
    options = options_from(args)
    stems = library_stems() if args.all_levels else [args.level]
    args.output.mkdir(parents=True)
    loader = SceneLoader()
    built, refused = [], []
    try:
        for stem in stems:
            try:
                item = build_one(loader, stem, args.output, args, options)
            except Unsupported as exc:
                refused.append({'level': stem, 'reason': str(exc)})
                print(f'{stem}: REFUSED {exc}', flush=True)
                continue
            built.append(item)
            print(json.dumps({k: item[k] for k in
                              ('level', 'triangles', 'instances', 'materials',
                               'textures', 'skipped', 'seconds')}), flush=True)
    finally:
        loader.close()
    report = {'parser': VERSION, 'source_identities': loader.identities,
              'export_options': options, 'levels': built, 'refused': refused,
              'levels_built': len(built), 'levels_refused': len(refused),
              'total_triangles': sum(item['triangles'] for item in built),
              'unresolved_texture_names': sorted({name for item in built
                                                  for name in item['unresolved_texture_names']}),
              'one_to_one_verified': False, 'reference_render_matched': False}
    with (args.output / 'world-report.json').open('w', encoding='utf-8') as handle:
        json.dump(report, handle, indent=2, allow_nan=False)
    print(json.dumps({k: v for k, v in report.items()
                      if k not in ('levels', 'source_identities', 'export_options')},
                     indent=2))
    if refused and not args.allow_partial:
        sys.exit(1)


if __name__ == '__main__':
    main()
