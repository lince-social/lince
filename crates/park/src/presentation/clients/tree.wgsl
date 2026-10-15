#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var client: texture_2d<f32>;
@group(1) @binding(1) var client_sampler: sampler;
struct Mapping {
    u: vec4<f32>,
    v: vec4<f32>,
}
@group(1) @binding(2) var<uniform> mapping: Mapping;

@fragment
fn fragment(input: UiVertexOutput) -> @location(0) vec4<f32> {
    let coordinates = vec3(1.0, input.uv);
    let uv = vec2(dot(mapping.u.xyz, coordinates), dot(mapping.v.xyz, coordinates));
    let pixel = textureSample(client, client_sampler, uv);
    let alpha = select(pixel.a, 1.0, mapping.u.w > 0.5);
    let rgb = clamp(pixel.rgb / max(alpha, 0.000001), vec3(0.0), vec3(1.0));
    let linear = select(rgb / 12.92, pow((rgb + 0.055) / 1.055, vec3(2.4)), rgb > vec3(0.04045));
    return vec4(linear, alpha);
}
