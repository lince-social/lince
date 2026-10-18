#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> inverse_x: vec4<f32>;
@group(1) @binding(1) var<uniform> inverse_y: vec4<f32>;
@group(1) @binding(2) var<uniform> inverse_w: vec4<f32>;
@group(1) @binding(3) var surface_image: texture_2d<f32>;
@group(1) @binding(4) var surface_sampler: sampler;

@fragment
fn fragment(input: UiVertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec3<f32>(input.uv, 1.0);
    let denominator = dot(inverse_w.xyz, pixel);
    let point = vec2<f32>(dot(inverse_x.xyz, pixel), dot(inverse_y.xyz, pixel)) / max(denominator, 0.000001);
    let color = textureSample(surface_image, surface_sampler, clamp(point, vec2<f32>(0.0), vec2<f32>(1.0)));
    if denominator <= 0.000001 || any(point < vec2<f32>(0.0)) || any(point > vec2<f32>(1.0)) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(color.rgb, 1.0);
}
