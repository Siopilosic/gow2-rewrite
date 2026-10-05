"""Blender stage: glTF in, FBX out, with a re-import comparison.

    blender --background --factory-startup --python-exit-code 1 \
            --python research/scripts/blender_export.py -- <level export directory>

The FBX is produced from the glTF this pipeline already wrote, so the two files
carry the same geometry, UVs, materials and texture set by construction, and the
glTF stays the single place where decode results are turned into a file. The
script then re-imports its own FBX and compares triangle counts, material
assignment, positions, UVs and image bindings, writing `fbx-validation.json`.

Axis settings: the export uses Blender's usual FBX convention (forward -Z, up Y),
which is what Unreal Engine 5 expects on import. That is an interchange
convention, not a claim about the game's own frame; the game frame is preserved
in `scene.gltf` when the build ran with `--axis raw`.
"""
import hashlib
import json
from pathlib import Path
import sys

import bpy

ROOT = Path(sys.argv[sys.argv.index('--') + 1]).resolve()
MANIFEST = json.loads((ROOT / 'manifest.json').read_text())


def mesh_objects():
    return [o for o in bpy.context.scene.objects if o.type == 'MESH']


def fingerprint(objects):
    """Per-triangle material name, corner positions and UVs, in scene order."""
    result = []
    for obj in objects:
        mesh = obj.data
        uv_layer = mesh.uv_layers.active
        for polygon in mesh.polygons:
            material = (mesh.materials[polygon.material_index].name.split('.')[0]
                        if mesh.materials else '')
            corners = []
            for loop_index in polygon.loop_indices:
                vertex = mesh.vertices[mesh.loops[loop_index].vertex_index]
                corners.append({
                    'position': list(obj.matrix_world @ vertex.co),
                    'uv': list(uv_layer.data[loop_index].uv) if uv_layer else [0.0, 0.0]})
            result.append({'material': material, 'corners': corners})
    return result


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'scene.gltf'))
imported = mesh_objects()
if not imported:
    raise SystemExit('glTF import produced no mesh')
before = fingerprint(imported)
if len(before) != MANIFEST['triangle_count']:
    raise SystemExit(f'glTF import triangle count {len(before)} != manifest '
                     f'{MANIFEST["triangle_count"]}')

for obj in imported:
    obj['warriors_level'] = MANIFEST['level']
    obj['warriors_worlds'] = ','.join(MANIFEST['worlds'])
    obj['warriors_one_to_one_verified'] = False
    obj['warriors_manifest'] = 'manifest.json'
    obj.select_set(True)
bpy.context.view_layer.objects.active = imported[0]

bpy.ops.export_scene.fbx(
    filepath=str(ROOT / 'scene.fbx'), use_selection=True, object_types={'MESH'},
    use_mesh_modifiers=False, add_leaf_bones=False, bake_anim=False,
    path_mode='COPY', embed_textures=False, use_custom_props=True,
    axis_forward='-Z', axis_up='Y', colors_type='LINEAR')

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.fbx(filepath=str(ROOT / 'scene.fbx'), use_custom_props=True,
                         colors_type='LINEAR')
after = fingerprint(mesh_objects())
if len(before) != len(after):
    raise SystemExit(f'FBX round trip changed the triangle count: '
                     f'{len(before)} -> {len(after)}')

worst = {'position': 0.0, 'uv': 0.0}
for source, target in zip(before, after):
    if source['material'] != target['material']:
        raise SystemExit('FBX round trip changed a material assignment')
    for a, b in zip(source['corners'], target['corners']):
        for key in worst:
            worst[key] = max(worst[key],
                             max(abs(i - j) for i, j in zip(a[key], b[key])))
if worst['position'] > 1e-3 or worst['uv'] > 1e-5:
    raise SystemExit(f'FBX round trip exceeded tolerance: {worst}')

images = {}
for material in bpy.data.materials:
    if not material.use_nodes:
        continue
    for node in material.node_tree.nodes:
        if node.type == 'TEX_IMAGE' and node.image:
            path = Path(bpy.path.abspath(node.image.filepath))
            if path.exists():
                images[material.name.split('.')[0]] = {
                    'file': path.name, 'size': list(node.image.size),
                    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

expected_images = {t['image'] for t in MANIFEST['textures']}
missing = sorted(name for name in expected_images
                 if not any(v['file'].startswith(name) for v in images.values()))

report = {
    'blender_version': bpy.app.version_string,
    'level': MANIFEST['level'],
    'source': 'scene.gltf', 'output': 'scene.fbx',
    'triangles': len(after),
    'materials': len({entry['material'] for entry in after}),
    'maximum_attribute_error': worst,
    'texture_images': images,
    'manifest_images': sorted(expected_images),
    'images_missing_after_roundtrip': missing,
    'fbx_blender_roundtrip': 'PASS',
    'fbx_sdk_validation': 'NOT_RUN', 'maya_validation': 'NOT_RUN',
    'unreal_import_validation': 'NOT_RUN',
    'axis_note': 'FBX written with forward -Z / up Y for interchange. The game '
                 'frame itself is unmodified in scene.gltf when built with --axis raw.',
    'one_to_one_verified': False, 'reference_render_matched': False}
(ROOT / 'fbx-validation.json').write_text(json.dumps(report, indent=2))
print(json.dumps({k: report[k] for k in
                  ('level', 'triangles', 'materials', 'maximum_attribute_error',
                   'images_missing_after_roundtrip', 'fbx_blender_roundtrip')}))
