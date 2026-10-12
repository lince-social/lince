import bpy
import math
from pathlib import Path
from mathutils import Vector, Quaternion

base = Path(__file__).resolve().parent
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
scene = bpy.context.scene
scene.render.fps = 24
scene.unit_settings.system = 'METRIC'
bpy.context.preferences.filepaths.save_version = 0

def material(name, color):
    result = bpy.data.materials.new(name)
    result.diffuse_color = (*color, 1)
    shader = result.node_tree.nodes.get('Principled BSDF')
    shader.inputs['Base Color'].default_value = (*color, 1)
    shader.inputs['Roughness'].default_value = 0.86
    return result

coat = material('Silver grey coat', (0.37, 0.43, 0.47))
light = material('Cream cheek ruff and paws', (0.78, 0.77, 0.66))
bib = material('Warm chest and muzzle', (0.88, 0.83, 0.69))
dark = material('Black ear tufts and tail tip', (0.025, 0.04, 0.05))
pink = material('Ear lining and tongue', (0.55, 0.30, 0.29))
iris = material('Olive eyes', (0.52, 0.65, 0.28))
white = material('Eye catchlights', (1, 1, 1))
outline_material = material('Fur outline', (0.10, 0.12, 0.14))
outline_material.use_backface_culling = True
motion = {}
for joint in ['head', 'ear_left', 'ear_right', 'front_left_paw', 'front_right_paw', 'hind_left_paw', 'hind_right_paw', 'tail']:
    motion[joint] = material('Motion:' + joint, (0.14, 0.13, 0.19))
parts = []

def finish(obj, name, mat, bone):
    obj.name = name
    obj.data.materials.append(mat)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    group = obj.vertex_groups.new(name=bone)
    group.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
    for polygon in obj.data.polygons:
        polygon.use_smooth = True
    parts.append(obj)
    return obj

def ellipsoid(name, position, scale, mat, bone):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=16, ring_count=10, location=position)
    obj = bpy.context.object
    obj.scale = scale
    return finish(obj, name, mat, bone)

def segment(name, start, end, radius, mat, bone, tip=None):
    delta = Vector(end) - Vector(start)
    bpy.ops.mesh.primitive_cone_add(vertices=10, radius1=radius, radius2=radius if tip is None else tip, depth=delta.length, location=(Vector(start) + Vector(end)) / 2)
    obj = bpy.context.object
    obj.rotation_euler = delta.to_track_quat('Z', 'Y').to_euler()
    return finish(obj, name, mat, bone)

armature = bpy.data.armatures.new('Fiote quadruped skeleton')
rig = bpy.data.objects.new('Fiote', armature)
rig['fiote_walk_stride'] = 0.18
rig['fiote_walk_stance'] = 0.65
scene.collection.objects.link(rig)
bpy.context.view_layer.objects.active = rig
rig.select_set(True)
bpy.ops.object.mode_set(mode='EDIT')

def bone(name, head, tail, parent=None):
    result = armature.edit_bones.new(name)
    result.head = head
    result.tail = tail
    if parent:
        result.parent = armature.edit_bones[parent]
    return result

bone('root', (0, 0, 0), (0, 0, 0.2))
bone('pelvis', (0, 0.3, 1.04), (0, 0, 1.03), 'root')
bone('spine', (0, 0, 1.03), (0, -0.35, 1.05), 'pelvis')
bone('chest', (0, -0.35, 1.05), (0, -0.45, 1.22), 'spine')
bone('neck', (0, -0.45, 1.22), (0, -0.58, 1.43), 'chest')
bone('head', (0, -0.58, 1.43), (0, -0.72, 1.63), 'neck')
bone('jaw', (0, -0.75, 1.37), (0, -0.98, 1.35), 'head')
bone('tongue', (0, -0.95, 1.35), (0, -1.10, 1.35), 'jaw')
bone('tail', (0, 0.64, 0.80), (0, 0.93, 0.82), 'pelvis')

for side, x in [('left', 0.28), ('right', -0.28)]:
    bone('ear_' + side, (x * 0.8, -0.58, 1.66), (x * 1.05, -0.55, 1.94), 'head')
    bone('eye_' + side, (x * 0.58, -0.88, 1.57), (x * 0.58, -0.92, 1.57), 'head')
    bone('lid_' + side, (x * 0.58, -0.87, 1.63), (x * 0.58, -0.87, 1.72), 'head')
    for limb, y, top, middle in [('front', -0.40, 1.03, 0.50), ('hind', 0.36, 1.08, 0.55)]:
        upper = limb + '_' + side
        lower = upper + '_lower'
        paw = upper + '_paw'
        bone(upper, (x, y, top), (x, y + 0.06, middle), 'chest' if limb == 'front' else 'pelvis')
        bone(lower, (x, y + 0.06, middle), (x, y - 0.02, 0.14), upper)
        bone(paw, (x, y - 0.02, 0.14), (x, y - 0.24, 0.10), lower)

bpy.ops.object.mode_set(mode='OBJECT')
rig.select_set(False)
ellipsoid('Body', (0, 0.14, 1.02), (0.34, 0.60, 0.36), coat, 'pelvis')
ellipsoid('Chest', (0, -0.38, 1.12), (0.31, 0.31, 0.39), coat, 'chest')
ellipsoid('Chest bib', (0, -0.62, 1.09), (0.23, 0.11, 0.31), bib, 'chest')
ellipsoid('Head', (0, -0.64, 1.55), (0.35, 0.28, 0.26), coat, 'head')
ellipsoid('Chin', (0, -0.83, 1.36), (0.19, 0.15, 0.085), bib, 'jaw')
ellipsoid('Tongue', (0, -0.94, 1.355), (0.045, 0.08, 0.014), pink, 'tongue')
ellipsoid('Nose', (0, -0.992, 1.47), (0.064, 0.04, 0.044), dark, 'head')
segment('Tail', (0, 0.64, 0.80), (0, 0.87, 0.82), 0.09, coat, 'tail', 0.07)
ellipsoid('Black tail tip', (0, 0.91, 0.82), (0.075, 0.10, 0.075), dark, 'tail')
segment('Tail motion seam', (0, 0.87, 0.82), (0, 0.885, 0.82), 0.074, motion['tail'], 'tail', 0.072)

for side, sign in [('left', 1), ('right', -1)]:
    x = sign * 0.28
    ellipsoid('Muzzle ' + side, (sign * 0.095, -0.89, 1.425), (0.12, 0.12, 0.09), bib, 'head')
    ellipsoid('Cheek ruff ' + side, (sign * 0.30, -0.68, 1.42), (0.115, 0.135, 0.12), light, 'head')
    for offset in range(3):
        segment('Cheek tuft', (sign * 0.31, -0.66, 1.48 - offset * 0.065), (sign * (0.48 - offset * 0.02), -0.68, 1.44 - offset * 0.08), 0.065, light, 'head', 0)
    ear = segment('Ear ' + side, (sign * 0.22, -0.58, 1.71), (sign * 0.30, -0.56, 1.95), 0.12, coat, 'ear_' + side, 0.012)
    ear.scale.y = 0.55
    segment('Ear lining ' + side, (sign * 0.22, -0.655, 1.74), (sign * 0.29, -0.585, 1.92), 0.065, pink, 'ear_' + side, 0.005)
    segment('Ear motion seam', (sign * 0.27, -0.65, 1.78), (sign * 0.30, -0.595, 1.92), 0.006, motion['ear_' + side], 'ear_' + side, 0.003)
    for offset in range(3):
        segment('Black ear tuft', (sign * 0.30, -0.56, 1.94), (sign * (0.30 + offset * 0.023), -0.56, 2.12 - offset * 0.015), 0.012, dark, 'ear_' + side, 0.002)
    ellipsoid('Eye rim ' + side, (sign * 0.165, -0.873, 1.59), (0.095, 0.043, 0.072), dark, 'head')
    ellipsoid('Eye ' + side, (sign * 0.165, -0.907, 1.59), (0.079, 0.018, 0.057), iris, 'eye_' + side)
    ellipsoid('Pupil ' + side, (sign * 0.165, -0.925, 1.59), (0.013, 0.009, 0.050), dark, 'eye_' + side)
    ellipsoid('Catchlight ' + side, (sign * 0.15, -0.934, 1.615), (0.011, 0.006, 0.012), white, 'eye_' + side)
    ellipsoid('Eyelid ' + side, (sign * 0.165, -0.906, 1.656), (0.085, 0.025, 0.017), coat, 'lid_' + side)
    segment('Brow motion seam', (sign * 0.12, -0.90, 1.66), (sign * 0.21, -0.875, 1.655), 0.004, motion['head'], 'head')
    for offset in range(3):
        segment('Whisker', (sign * 0.12, -0.95, 1.44 - offset * 0.02), (sign * 0.42, -1.0, 1.47 - offset * 0.05), 0.003, light, 'head')
    for limb, y, top, middle in [('front', -0.40, 1.03, 0.50), ('hind', 0.36, 1.08, 0.55)]:
        upper = limb + '_' + side
        lower = upper + '_lower'
        paw = upper + '_paw'
        ellipsoid(upper, (x, y, (top + middle) / 2), (0.115 if limb == 'front' else 0.18, 0.15, (top - middle) / 2 + 0.08), coat, upper)
        ellipsoid(lower, (x, y + 0.02, (middle + 0.14) / 2), (0.095, 0.10, (middle - 0.14) / 2 + 0.06), coat, lower)
        ellipsoid(paw, (x, y - 0.13, 0.11), (0.165, 0.23, 0.11), light, paw)
        ellipsoid('Paw motion seam', (x, y - 0.14, 0.205), (0.12, 0.13, 0.008), motion[paw], paw)
        for toe in [-1, 0, 1]:
            ellipsoid('Toe', (x + toe * 0.075, y - 0.28, 0.105), (0.058, 0.08, 0.075), light, paw)

surface_names = {'Body', 'Chest', 'Head'}
for limb in ['front', 'hind']:
    for side in ['left', 'right']:
        surface_names.update([limb + '_' + side, limb + '_' + side + '_lower', limb + '_' + side + '_paw'])
surface_parts = [obj for obj in parts if obj.name in surface_names]
details = [obj for obj in parts if obj.name not in surface_names]
bpy.ops.object.select_all(action='DESELECT')
for obj in surface_parts:
    obj.select_set(True)
bpy.context.view_layer.objects.active = surface_parts[0]
bpy.ops.object.join()
surface = bpy.context.object
surface.data.materials.clear()
surface.data.materials.append(coat)
surface.vertex_groups.clear()
remesh = surface.modifiers.new('Continuous coat', 'REMESH')
remesh.mode = 'VOXEL'
remesh.voxel_size = 0.027
remesh.use_smooth_shade = True
bpy.ops.object.modifier_apply(modifier=remesh.name)
smooth = surface.modifiers.new('Soft anatomy', 'SMOOTH')
smooth.factor = 0.8
smooth.iterations = 3
bpy.ops.object.modifier_apply(modifier=smooth.name)
decimate = surface.modifiers.new('Portrait mesh budget', 'DECIMATE')
decimate.ratio = 0.32
bpy.ops.object.modifier_apply(modifier=decimate.name)
deform = [item for item in armature.bones if item.name in surface_names or item.name in ['pelvis', 'spine', 'chest', 'neck', 'head']]
groups = {item.name: surface.vertex_groups.new(name=item.name) for item in deform}
for vertex in surface.data.vertices:
    point = surface.matrix_world @ vertex.co
    distances = []
    for item in deform:
        axis = item.tail_local - item.head_local
        phase = max(0, min(1, (point - item.head_local).dot(axis) / axis.length_squared))
        distance = (point - item.head_local - axis * phase).length
        distances.append((distance, item.name))
    distances.sort()
    selected = distances[:3]
    weights = [1 / max(distance, 0.04) ** 4 for distance, name in selected]
    total = sum(weights)
    for (distance, name), weight in zip(selected, weights):
        groups[name].add([vertex.index], weight / total, 'REPLACE')
outline = surface.copy()
outline.data = surface.data.copy()
outline.name = 'Fur silhouette'
scene.collection.objects.link(outline)
outline.data.materials.clear()
outline.data.materials.append(outline_material)
for vertex in outline.data.vertices:
    vertex.co += vertex.normal * 0.014
for polygon in outline.data.polygons:
    polygon.flip()
details.append(outline)
bpy.ops.object.select_all(action='DESELECT')
surface.select_set(True)
for obj in details:
    obj.select_set(True)
bpy.context.view_layer.objects.active = surface
bpy.ops.object.join()
mesh = bpy.context.object
mesh.name = 'Fiote skinned coat'
mesh.parent = rig
modifier = mesh.modifiers.new('Skeleton', 'ARMATURE')
modifier.object = rig
rig.animation_data_create()

bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.select_all(action='SELECT')
bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.025)
bpy.ops.object.mode_set(mode='OBJECT')
fur = bpy.data.images.new('Fiote neutral fur grain', width=128, height=128)
pixels = []
for y in range(128):
    for x in range(128):
        grain = 0.94 + 0.04 * math.sin(x * 1.17 + math.sin(y * 0.24)) * math.sin(y * 0.37 + x * 0.09)
        pixels.extend((grain, grain, grain, 1))
fur.pixels.foreach_set(pixels)
fur.pack()
texture = coat.node_tree.nodes.new('ShaderNodeTexImage')
texture.image = fur
multiply = coat.node_tree.nodes.new('ShaderNodeMixRGB')
multiply.blend_type = 'MULTIPLY'
multiply.inputs[0].default_value = 1
multiply.inputs[2].default_value = coat.diffuse_color
coat.node_tree.links.new(texture.outputs['Color'], multiply.inputs[1])
coat.node_tree.links.new(multiply.outputs['Color'], coat.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])

bpy.context.view_layer.objects.active = rig
mesh.select_set(False)
rig.select_set(True)
bpy.ops.object.mode_set(mode='EDIT')
for side, x in [('left', 0.28), ('right', -0.28)]:
    for limb, y, top in [('front', -0.40, 1.03), ('hind', 0.36, 1.08)]:
        name = limb + '_' + side
        control = bone('ctrl_' + name, (x, y - 0.02, 0.14), (x, y - 0.24, 0.10))
        control.use_deform = False
        pole = bone('pole_' + name, (x, y + (0.65 if limb == 'front' else -0.65), 0.50), (x, y + (0.65 if limb == 'front' else -0.65), 0.65))
        pole.use_deform = False
bpy.ops.object.mode_set(mode='OBJECT')
for side in ['left', 'right']:
    for limb in ['front', 'hind']:
        name = limb + '_' + side
        constraint = rig.pose.bones[name + '_lower'].constraints.new('IK')
        constraint.name = 'Planted paw'
        constraint.target = rig
        constraint.subtarget = 'ctrl_' + name
        constraint.pole_target = rig
        constraint.pole_subtarget = 'pole_' + name
        constraint.pole_angle = math.pi / 2
        constraint.chain_count = 2
        constraint.use_stretch = False
        constraint = rig.pose.bones[name + '_paw'].constraints.new('COPY_ROTATION')
        constraint.name = 'Paw orientation'
        constraint.target = rig
        constraint.subtarget = 'ctrl_' + name
        constraint.target_space = 'WORLD'
        constraint.owner_space = 'WORLD'

def paw(name, position):
    item = rig.pose.bones['ctrl_' + name]
    item.location = item.bone.matrix_local.to_3x3().inverted() @ (Vector(position) - item.bone.head_local)

def aim(name, target, strength):
    bpy.context.view_layer.update()
    item = rig.pose.bones[name]
    forward = item.matrix.to_3x3() @ item.bone.matrix_local.to_3x3().inverted() @ Vector((0, -1, 0))
    direction = Vector(target) - item.head
    turn = Quaternion().slerp(forward.rotation_difference(direction), strength)
    transformed = turn.to_matrix().to_4x4() @ item.matrix
    transformed.translation = item.head
    item.matrix = transformed

def reset():
    for p in rig.pose.bones:
        p.rotation_mode = 'XYZ'
        p.rotation_euler = (0, 0, 0)
        p.location = (0, 0, 0)
        p.scale = (1, 1, 1)

def posture(kind):
    if kind == 'sit':
        rig.pose.bones['pelvis'].rotation_euler.x = -0.50
        rig.pose.bones['root'].location.y = -0.31
        rig.pose.bones['neck'].rotation_euler.x = 0.50
        for side in ['left', 'right']:
            x = 0.28 if side == 'left' else -0.28
            paw('front_' + side, (x, -0.40, 0.14))
            paw('hind_' + side, (x, 0.12, 0.14))
    if kind == 'rest':
        rig.pose.bones['root'].location.y = -0.56
        rig.pose.bones['neck'].rotation_euler.x = 0.18
        for side in ['left', 'right']:
            x = 0.28 if side == 'left' else -0.28
            paw('front_' + side, (x, -0.72, 0.14))
            paw('hind_' + side, (x, 0.38, 0.14))

def pose(kind, phase):
    p = rig.pose.bones
    p['root'].location.y = -0.03
    if kind in ['sit_idle', 'groom_paw']:
        posture('sit')
    if kind in ['rest', 'sleep', 'groom_flank']:
        posture('rest')
    p['chest'].scale = (1, 1, 1 + 0.014 * math.sin(phase * math.tau))
    blink = max(0, 1 - abs(phase - 0.45) / 0.04)
    for side in ['left', 'right']:
        p['lid_' + side].location.y = -0.16 * blink
        p['lid_' + side].scale = (1, 1 + 3.6 * blink, 1 + blink)
    if kind in ['idle', 'sit_idle', 'look_around']:
        p['head'].rotation_euler.y = 0.13 * math.sin(phase * math.tau)
        p['ear_left'].rotation_euler.z = 0.12 * math.sin(phase * math.tau * 2)
        p['ear_right'].rotation_euler.z = -0.08 * math.sin(phase * math.tau)
    if kind == 'walk':
        for limb, offset in [('front_left', 0), ('front_right', 0.5), ('hind_left', 0.25), ('hind_right', 0.75)]:
            step = (phase + offset) % 1
            x = 0.28 if limb.endswith('left') else -0.28
            base_y = -0.42 if limb.startswith('front') else 0.34
            if step <= 0.65:
                y = base_y - 0.09 + 0.18 * step / 0.65
                z = 0.14
            else:
                swing = (step - 0.65) / 0.35
                y = base_y + 0.09 - 0.18 * swing * swing * (3 - 2 * swing)
                z = 0.14 + 0.08 * math.sin(swing * math.pi)
            paw(limb, (x, y, z))
        p['root'].location.y = -0.03 + 0.012 * math.sin(phase * math.tau * 2)
    if kind in ['turn_left', 'turn_right']:
        pose('walk', phase)
        p['head'].rotation_euler.y = 0.25 if kind == 'turn_left' else -0.25
        sign = 1 if kind == 'turn_left' else -1
        for limb, offset in [('front_left', 0), ('front_right', 0.5), ('hind_left', 0.25), ('hind_right', 0.75)]:
            step = (phase + offset) % 1
            half = 0.55 * 1.5 * 0.65 / 2
            if step <= 0.65:
                angle = sign * (half - 0.55 * 1.5 * step)
                z = 0.14
            else:
                swing = (step - 0.65) / 0.35
                angle = sign * (-half + 2 * half * swing * swing * (3 - 2 * swing))
                z = 0.14 + 0.08 * math.sin(swing * math.pi)
            x = 0.28 if limb.endswith('left') else -0.28
            y = -0.42 if limb.startswith('front') else 0.34
            paw(limb, (x * math.cos(angle) - y * math.sin(angle), x * math.sin(angle) + y * math.cos(angle), z))
    if kind == 'groom_paw':
        paw('front_left', (0.18, -0.70, 1.10))
        p['neck'].rotation_euler.x = 0.80
        p['head'].rotation_euler.z = -0.35
        p['jaw'].rotation_euler.x = 0.12 * (1 + math.sin(phase * math.tau * 4))
        p['tongue'].location.y = 0.06 * (1 + math.sin(phase * math.tau * 4))
    if kind == 'groom_flank':
        aim('neck', (0.36, -0.03, 0.58), 0.45)
        aim('head', (0.36, -0.03, 0.58), 0.85)
        p['jaw'].rotation_euler.x = 0.1 * (1 + math.sin(phase * math.tau * 4))
        p['tongue'].location.y = 0.06 * (1 + math.sin(phase * math.tau * 4))
    if kind == 'stretch':
        envelope = math.sin(phase * math.pi)
        p['root'].location.y = -0.03 - 0.15 * envelope
        p['spine'].rotation_euler.x = 0.35 * envelope
        p['neck'].rotation_euler.x = -0.3 * envelope
        for side in ['left', 'right']:
            x = 0.28 if side == 'left' else -0.28
            paw('front_' + side, (x, -0.42 - 0.18 * envelope, 0.14))
    if kind == 'sleep':
        p['head'].rotation_euler.x = 0.30
        for side in ['left', 'right']:
            p['lid_' + side].location.y = -0.16
            p['lid_' + side].scale = (1, 4.6, 2)

durations = {'idle': 4, 'look_around': 4, 'walk': 1.5, 'turn_left': 1.5, 'turn_right': 1.5, 'sit_idle': 4, 'sit_down': 1.5, 'stand_up': 1.5, 'lie_down': 2, 'rest': 4, 'get_up': 2, 'fall_asleep': 2, 'sleep': 6, 'wake_up': 2, 'groom_paw': 4, 'groom_flank': 4, 'stretch': 3}
transitions = {'sit_down': ('idle', 'sit_idle'), 'stand_up': ('sit_idle', 'idle'), 'lie_down': ('idle', 'rest'), 'get_up': ('rest', 'idle'), 'fall_asleep': ('rest', 'sleep'), 'wake_up': ('sleep', 'rest')}

for name, duration in durations.items():
    action = bpy.data.actions.new(name)
    rig.animation_data.action = action
    end = round(duration * scene.render.fps / 2) * 2
    for frame in range(0, end + 1):
        phase = frame / end
        reset()
        if name in transitions:
            start_kind, end_kind = transitions[name]
            pose(start_kind, 0)
            first = {p.name: (p.location.copy(), p.rotation_euler.copy(), p.scale.copy()) for p in rig.pose.bones}
            reset()
            pose(end_kind, 0)
            amount = phase * phase * (3 - 2 * phase)
            for p in rig.pose.bones:
                location, rotation, scale = first[p.name]
                p.location = location.lerp(p.location, amount)
                p.rotation_euler = tuple(rotation[i] * (1 - amount) + p.rotation_euler[i] * amount for i in range(3))
                p.scale = scale.lerp(p.scale, amount)
        else:
            pose(name, phase)
        for p in rig.pose.bones:
            p.keyframe_insert(data_path='location', frame=frame)
            p.keyframe_insert(data_path='rotation_euler', frame=frame)
            p.keyframe_insert(data_path='scale', frame=frame)
    for slot in action.slots:
        for layer in action.layers:
            for strip in layer.strips:
                bag = strip.channelbag(slot)
                if bag:
                    for curve in bag.fcurves:
                        for key in curve.keyframe_points:
                            key.interpolation = 'LINEAR'
    track = rig.animation_data.nla_tracks.new()
    track.name = name
    track.strips.new(name, 0, action)
    track.mute = True

rig.animation_data.action = None
reset()
scene.frame_set(0)
bpy.ops.object.select_all(action='DESELECT')
mesh.select_set(True)
rig.select_set(True)
bpy.context.view_layer.objects.active = rig
bpy.ops.export_scene.gltf(filepath=str(base / 'fiote.glb'), export_format='GLB', use_selection=True, export_animations=True, export_animation_mode='NLA_TRACKS', export_skins=True, export_def_bones=True, export_force_sampling=True, export_extras=True)

reset()
posture('sit')
scene.render.engine = 'BLENDER_EEVEE'
scene.render.resolution_x = 512
scene.render.resolution_y = 512
scene.render.resolution_percentage = 100
scene.render.film_transparent = True
scene.world.color = (0.25, 0.25, 0.25)
camera_data = bpy.data.cameras.new('Seated portrait')
camera = bpy.data.objects.new('Seated portrait', camera_data)
scene.collection.objects.link(camera)
camera.location = (0, -5, 1.6)
camera.rotation_euler = (Vector((0, -0.05, 0.98)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
camera_data.type = 'ORTHO'
camera_data.ortho_scale = 2.65
scene.camera = camera
for name, position, energy, size in [('Key', (-3, -4, 5), 450, 5), ('Fill', (3, -2, 3), 250, 4)]:
    data = bpy.data.lights.new(name, 'AREA')
    data.energy = energy
    data.shape = 'DISK'
    data.size = size
    obj = bpy.data.objects.new(name, data)
    scene.collection.objects.link(obj)
    obj.location = position
    obj.rotation_euler = (Vector((0, 0, 1)) - obj.location).to_track_quat('-Z', 'Y').to_euler()
scene.render.filepath = str(base / 'fiote-seated.png')
bpy.ops.wm.save_as_mainfile(filepath=str(base / 'fiote.blend'))
bpy.ops.render.render(write_still=True)
