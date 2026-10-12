import bpy
import json
import sys
from pathlib import Path

rig = bpy.data.objects['Fiote']
scene = bpy.context.scene
mesh = bpy.data.objects['Fiote skinned coat']
names = ['idle', 'look_around', 'walk', 'turn_left', 'turn_right', 'sit_idle', 'sit_down', 'stand_up', 'lie_down', 'rest', 'get_up', 'fall_asleep', 'sleep', 'wake_up', 'groom_paw', 'groom_flank', 'stretch']
assert all(name in bpy.data.actions for name in names)
assert all('ctrl_' + limb in rig.pose.bones for limb in ['front_left', 'front_right', 'hind_left', 'hind_right'])
blended = 0
for vertex in mesh.data.vertices:
    total = sum(group.weight for group in vertex.groups)
    assert abs(total - 1) < 0.001
    blended += len(vertex.groups) > 1
maximum_contact_error = 0
loops = ['idle', 'walk', 'turn_left', 'turn_right', 'sit_idle', 'rest', 'sleep']
for name in names:
    rig.animation_data.action = bpy.data.actions[name]
    end = round(rig.animation_data.action.frame_range[1])
    first = None
    for frame in range(end + 1):
        scene.frame_set(frame)
        bpy.context.view_layer.update()
        positions = {}
        for limb in ['front_left', 'front_right', 'hind_left', 'hind_right']:
            paw = rig.pose.bones[limb + '_paw']
            control = rig.pose.bones['ctrl_' + limb]
            error = (paw.head - control.head).length
            assert error < 0.03, (name, frame, limb, error)
            assert paw.head.z > 0.09, (name, frame, limb, paw.head.z)
            maximum_contact_error = max(maximum_contact_error, error)
            positions[limb] = paw.head.copy()
        if frame == 0:
            first = positions
        if name in loops and frame == end:
            for limb in first:
                assert (first[limb] - positions[limb]).length < 0.001, (name, limb, 'loop seam')
result = {'vertices': len(mesh.data.vertices), 'triangles': sum(len(polygon.vertices) - 2 for polygon in mesh.data.polygons), 'blended_vertices': blended, 'maximum_paw_target_error_m': maximum_contact_error, 'animation_clips': len(names), 'ik_chains': 4, 'fur_texture_size': [128, 128]}
assert blended > 1000
output = Path(sys.argv[sys.argv.index('--') + 1]) if '--' in sys.argv else Path('target/fiote-refinement/asset-validation.json')
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
