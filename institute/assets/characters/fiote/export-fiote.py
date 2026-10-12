import bpy
from pathlib import Path

base = Path(__file__).resolve().parent
if Path(bpy.data.filepath).resolve() != base / 'fiote.blend':
    bpy.ops.wm.open_mainfile(filepath=str(base / 'fiote.blend'))
rig = bpy.data.objects['Fiote']
rig.animation_data.action = None
for pose in rig.pose.bones:
    pose.location = (0, 0, 0)
    pose.rotation_mode = 'XYZ'
    pose.rotation_euler = (0, 0, 0)
    pose.scale = (1, 1, 1)
bpy.ops.object.select_all(action='DESELECT')
rig.select_set(True)
for obj in bpy.data.objects:
    if obj.type == 'MESH' and any(modifier.type == 'ARMATURE' and modifier.object == rig for modifier in obj.modifiers):
        obj.select_set(True)
bpy.context.view_layer.objects.active = rig
bpy.ops.export_scene.gltf(filepath=str(base / 'fiote.glb'), export_format='GLB', use_selection=True, export_animations=True, export_animation_mode='NLA_TRACKS', export_skins=True, export_def_bones=True, export_force_sampling=True, export_extras=True)
rig.animation_data.action = bpy.data.actions['sit_idle']
bpy.context.scene.frame_set(0)
bpy.context.scene.render.film_transparent = True
bpy.context.scene.render.filepath = str(base / 'fiote-seated.png')
bpy.ops.render.render(write_still=True)
