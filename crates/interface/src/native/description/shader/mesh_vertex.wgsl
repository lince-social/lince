#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world, mesh_position_local_to_clip, mesh_normal_local_to_world}
#import bevy_pbr::mesh_bindings::mesh

struct LinceVertexInput {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<storage, read> lince_geometry: array<MeshVertex>;

@vertex
fn lince_vertex(input: LinceVertexInput) -> LinceVaryings {
    var original = MeshVertex(input.position, input.normal, input.uv);
    if LINCE_GENERATED_GEOMETRY {
        original = lince_geometry[input.vertex_index - mesh[input.instance_index].first_vertex_index];
    }
    let changed = deform_vertex(original, shader_inputs);
    let delta = changed.position - original.position;
    let finite_delta = select(vec3<f32>(0.0), delta, all(abs(delta) <= vec3<f32>(1000000.0)));
    let displacement = finite_delta * min(1.0, lince_parameters.x / max(length(finite_delta), 0.000001));
    let position = original.position + displacement;
    let finite_normal = select(original.normal, changed.normal, all(abs(changed.normal) <= vec3<f32>(1000000.0)));
    let normal = select(original.normal, finite_normal, length(finite_normal) > 0.000001);
    let uv = select(original.uv, clamp(changed.uv, vec2<f32>(0.0), vec2<f32>(1.0)), all(abs(changed.uv) <= vec2<f32>(1000000.0)));
    let world_from_local = get_world_from_local(input.instance_index);
    var output: LinceVaryings;
    output.position = mesh_position_local_to_clip(world_from_local, vec4<f32>(position, 1.0));
    output.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(position, 1.0)).xyz;
    output.normal = mesh_normal_local_to_world(normalize(normal), input.instance_index);
    output.uv = uv;
    output.local_position = position;
    output.instance_index = input.instance_index;
    return output;
}
