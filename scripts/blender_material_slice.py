"""Run with Blender --background --python this_file -- <slice directory>.

Real FBX export/re-import plus an explicit unlit inspection adapter, not PS2 shading.
"""
import bpy
import hashlib
import json
import math
import sys
from pathlib import Path
from mathutils import Vector, Matrix

root=Path(sys.argv[sys.argv.index('--')+1]).resolve()
report=json.loads((root/'scene.warriors.json').read_text())
corners=report['corners']
bpy.ops.wm.read_factory_settings(use_empty=True)
mesh=bpy.data.meshes.new('Warriors_6868_slot0')
mesh.from_pydata([c['position'] for c in corners],[],[tuple(range(i,i+3)) for i in range(0,len(corners),3)])
mesh.update()
obj=bpy.data.objects.new('Warriors_6868_slot0',mesh);bpy.context.collection.objects.link(obj)
obj.location=report.get('instance',{}).get('translation',(0,0,0))
obj['warriors_source_member']=6868
obj['warriors_material_sidecar']='scene.warriors.json'
obj['warriors_reference_render_matched']=False
uv=mesh.uv_layers.new(name='SourceUV')
colors=mesh.color_attributes.new(name='SourceRGBA',type='FLOAT_COLOR',domain='CORNER')
for loop in mesh.loops:
    c=corners[loop.vertex_index]
    uv.data[loop.index].uv=(c['uv'][0],1-c['uv'][1])
    colors.data[loop.index].color=[v/255 for v in c['raw_inputs']['2']]
normals=[]
for c in corners:
    n=Vector(c['raw_inputs']['3'][:3]);normals.append(tuple(n.normalized()) if n.length else (0,0,0))
mesh.normals_split_custom_set(normals)
for m in report['materials']:
    material=bpy.data.materials.new(m['name']);material.use_nodes=True
    shader=material.node_tree.nodes.get('Principled BSDF')
    texture=material.node_tree.nodes.new('ShaderNodeTexImage')
    texture.image=bpy.data.images.load(str(root/report['textures'][m['name']]['image']))
    texture.interpolation='Linear';texture.extension='REPEAT'
    material.node_tree.links.new(texture.outputs['Color'],shader.inputs['Base Color'])
    material['warriors_scope']='Inspection material; original GS state is in sidecar'
    mesh.materials.append(material)
for polygon in mesh.polygons:polygon.material_index=corners[polygon.vertices[0]]['material']
obj.select_set(True);bpy.context.view_layer.objects.active=obj


def manifest(obj):
    result=[];m=obj.data
    for p in m.polygons:
        result.append({'material':m.materials[p.material_index].name.split('.')[0],
            'corners':[{'position':list(obj.matrix_world @ m.vertices[m.loops[i].vertex_index].co),
                        'uv':list(m.uv_layers.active.data[i].uv),
                        'normal':list(m.corner_normals[i].vector),
                        'color':list(m.color_attributes.active_color.data[i].color) if m.color_attributes.active_color else None}
                       for i in p.loop_indices]})
    return result


bpy.context.view_layer.update()
before=manifest(obj)
bpy.ops.export_scene.fbx(filepath=str(root/'scene.fbx'),use_selection=True,object_types={'MESH'},
    use_mesh_modifiers=False,add_leaf_bones=False,bake_anim=False,path_mode='RELATIVE',
    use_custom_props=True,axis_forward='-Z',axis_up='Y',colors_type='LINEAR')
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.fbx(filepath=str(root/'scene.fbx'),use_custom_props=True,colors_type='LINEAR')
objects=[o for o in bpy.context.scene.objects if o.type=='MESH']
assert len(objects)==1,'mesh instance count changed'
obj=objects[0];after=manifest(obj)
assert len(before)==len(after),'polygon count changed'
maximum={'position':0.,'uv':0.,'normal':0.,'color':0.}
for a,b in zip(before,after):
    assert a['material']==b['material'],'material assignment changed'
    assert len(a['corners'])==len(b['corners'])==3
    for x,y in zip(a['corners'],b['corners']):
        for key in maximum:
            assert x[key] is not None and y[key] is not None,f'{key} missing'
            maximum[key]=max(maximum[key],max(abs(i-j) for i,j in zip(x[key],y[key])))
assert maximum['position']<1e-4 and maximum['uv']<1e-5 and maximum['normal']<1e-3 and maximum['color']<1e-5,maximum
texture_links={}
for mat in obj.data.materials:
    shader=next(n for n in mat.node_tree.nodes if n.type=='BSDF_PRINCIPLED')
    image_nodes=[link.from_node for link in shader.inputs['Base Color'].links if link.from_node.type=='TEX_IMAGE' and link.from_node.image]
    assert len(image_nodes)==1,'texture connection missing or ambiguous'
    tex=image_nodes[0];expected=report['textures'][mat.name.split('.')[0]]
    assert tuple(tex.image.size)==(expected['width'],expected['height']),'image dimensions changed'
    path=Path(bpy.path.abspath(tex.image.filepath)).resolve()
    assert path==root/expected['image'],'media path changed'
    assert hashlib.sha256(path.read_bytes()).hexdigest()==expected['image_sha256'],'media contents changed'
    texture_links[mat.name]=str(path.relative_to(root))
    # Explicit post-import inspection adapter. Standard FBX material remains portable.
    output=next(n for n in mat.node_tree.nodes if n.type=='OUTPUT_MATERIAL')
    emission=mat.node_tree.nodes.new('ShaderNodeEmission')
    mat.node_tree.links.new(tex.outputs['Color'],emission.inputs['Color'])
    mat.node_tree.links.new(emission.outputs[0],output.inputs['Surface'])

scene=bpy.context.scene
scene.render.engine='CYCLES';scene.cycles.samples=16
scene.render.resolution_x=1000;scene.render.resolution_y=800;scene.render.resolution_percentage=100
scene.render.image_settings.file_format='PNG'
scene.world=bpy.data.worlds.new('Inspection background');scene.world.color=(.035,.025,.02)
points=[obj.matrix_world@v.co for v in obj.data.vertices]
lo=Vector([min(p[k] for p in points) for k in range(3)]);hi=Vector([max(p[k] for p in points) for k in range(3)])
center=(lo+hi)/2;radius=(hi-lo).length/2
camera=bpy.data.cameras.new('InspectionCamera');cam=bpy.data.objects.new('InspectionCamera',camera)
scene.collection.objects.link(cam);scene.camera=cam
rotation=Matrix.Rotation(math.radians(-32),4,'Y')@Matrix.Rotation(math.radians(-38),4,'X')
cam.location=center+rotation.to_3x3()@Vector((0,0,radius*2.7))
cam.rotation_euler=rotation.to_euler();camera.clip_end=radius*20
scene.view_settings.view_transform='Standard';scene.view_settings.look='None'
scene.render.filepath=str(root/'fbx-reimport.png')
bpy.ops.wm.save_as_mainfile(filepath=str(root/'inspection.blend'))
bpy.ops.render.render(write_still=True)
verification={'blender_version':bpy.app.version_string,'triangles':len(after),'maximum_attribute_error':maximum,
              'texture_links':texture_links,'fbx_blender_roundtrip':'PASS','fbx_sdk_validation':'NOT_RUN',
              'maya_validation':'NOT_RUN','reference_render_matched':False,
              'adapter':'Unlit texture inspection; not original PS2 shader reconstruction'}
(root/'fbx-validation.json').write_text(json.dumps(verification,indent=2))
print(json.dumps(verification))
