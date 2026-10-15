@group(0) @binding(1) var<uniform> lince_counts: vec4<u32>;
@group(0) @binding(2) var<storage, read_write> lince_vertices: array<MeshVertex>;

@compute @workgroup_size(64)
fn lince_generate(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= lince_counts.x {
        return;
    }
    let changed = generate_vertex(invocation.x, lince_counts.x, shader_inputs);
    let position = select(vec3<f32>(0.0), clamp(changed.position, vec3<f32>(-0.5), vec3<f32>(0.5)), all(abs(changed.position) <= vec3<f32>(1000000.0)));
    let finite_normal = select(vec3<f32>(0.0, 0.0, 1.0), changed.normal, all(abs(changed.normal) <= vec3<f32>(1000000.0)));
    let normal = select(vec3<f32>(0.0, 0.0, 1.0), finite_normal, length(finite_normal) > 0.000001);
    let uv = select(vec2<f32>(0.0), clamp(changed.uv, vec2<f32>(0.0), vec2<f32>(1.0)), all(abs(changed.uv) <= vec2<f32>(1000000.0)));
    lince_vertices[invocation.x] = MeshVertex(position, normalize(normal), uv);
}
