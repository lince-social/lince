#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var client: texture_2d<f32>;
@group(1) @binding(1) var client_sampler: sampler;

@fragment
fn fragment(input: UiVertexOutput) -> @location(0) vec4<f32> {
    let pixel = textureSample(client, client_sampler, input.uv);
    let rgb = clamp(pixel.rgb / max(pixel.a, 0.000001), vec3(0.0), vec3(1.0));
    let linear = select(rgb / 12.92, pow((rgb + 0.055) / 1.055, vec3(2.4)), rgb > vec3(0.04045));
    return vec4(linear, pixel.a);
}
