#import bevy_pbr::{
    mesh_functions::get_local_from_world,
    mesh_view_bindings::view,
    prepass_utils::prepass_depth,
    view_transformations::{frag_coord_to_ndc, position_ndc_to_world},
}

@fragment
fn lince_fragment(input: LinceVaryings, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let local_from_world = get_local_from_world(input.instance_index);
    let eye = (local_from_world * vec4<f32>(view.world_position, 1.0)).xyz;
    let orthographic = view.clip_from_view[3].w > 0.5;
    let parallel = normalize((local_from_world * vec4<f32>(-view.world_from_view[2].xyz, 0.0)).xyz);
    let direction = normalize(select(input.local_position - eye, parallel, orthographic));
    let near_world = position_ndc_to_world(frag_coord_to_ndc(vec4<f32>(input.position.xy, 1.0, 1.0)));
    let origin = (local_from_world * vec4<f32>(near_world, 1.0)).xyz;
    let reciprocal = select(vec3<f32>(-1.0), vec3<f32>(1.0), direction >= vec3<f32>(0.0)) / max(abs(direction), vec3<f32>(0.000001));
    let first = (vec3<f32>(-0.5) - origin) * reciprocal;
    let last = (vec3<f32>(0.5) - origin) * reciprocal;
    let near_axes = min(first, last);
    let far_axes = max(first, last);
    let entry = max(max(near_axes.x, near_axes.y), near_axes.z);
    var exit = min(min(far_axes.x, far_axes.y), far_axes.z);
    if front == (entry < 0.0) {
        discard;
    }
    let scene_depth = prepass_depth(input.position, 0u);
    if scene_depth > 0.0000001 {
        let scene_world = position_ndc_to_world(frag_coord_to_ndc(vec4<f32>(input.position.xy, scene_depth, 1.0)));
        let scene_local = (local_from_world * vec4<f32>(scene_world, 1.0)).xyz;
        exit = min(exit, dot(scene_local - origin, direction));
    }
    let start = max(entry, 0.0);
    if exit <= start {
        discard;
    }
    let step_length = (exit - start) / f32(LINCE_VOLUME_STEPS);
    var accumulated = vec4<f32>(0.0);
    for (var index = 0u; index < LINCE_VOLUME_STEPS; index += 1u) {
        let position = origin + direction * (start + (f32(index) + 0.5) * step_length);
        let raw = volume_sample(position, shader_inputs);
        let sample = select(vec4<f32>(0.0), clamp(raw, vec4<f32>(0.0), vec4<f32>(1.0)), all(abs(raw) <= vec4<f32>(1000000.0)));
        let opacity = 1.0 - exp(-clamp(lince_parameters.y, 0.0, 8.0) * sample.a * step_length);
        let contribution = (1.0 - accumulated.a) * opacity;
        accumulated = vec4<f32>(accumulated.rgb + sample.rgb * contribution, accumulated.a + contribution);
        if accumulated.a >= 0.995 {
            break;
        }
    }
    let strength = clamp(shader_inputs.effect_strength, 0.0, 1.0);
#ifdef LINCE_VOLUME_PREVIEW
    return vec4<f32>(accumulated.rgb / max(accumulated.a, 0.000001), accumulated.a * strength);
#else
    return accumulated * strength;
#endif
}
