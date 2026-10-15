#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var surface: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var surface_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(surface, surface_sampler, in.uv);
    let normal = normalize(in.world_normal);
    let light = 0.35 + 0.65 * max(dot(normal, normalize(vec3(-0.5, 1.0, 0.7))), 0.0);
    return vec4(color.rgb * light, 1.0);
}
