#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var client: texture_2d<f32>;
@group(1) @binding(1) var client_sampler: sampler;
struct Mapping {
    u: vec4<f32>,
    v: vec4<f32>,
    color_red: vec4<f32>,
    color_green: vec4<f32>,
    color_blue: vec4<f32>,
    color_curve: vec4<f32>,
}
@group(1) @binding(2) var<uniform> mapping: Mapping;

@fragment
fn fragment(input: UiVertexOutput) -> @location(0) vec4<f32> {
    let coordinates = vec3(1.0, input.uv);
    let uv = vec2(dot(mapping.u.xyz, coordinates), dot(mapping.v.xyz, coordinates));
    let pixel = textureSample(client, client_sampler, uv);
    let alpha = select(pixel.a, 1.0, mapping.u.w > 0.5);
    var rgb = pixel.rgb / max(alpha, 0.000001);
    if mapping.color_curve.x < 2.5 { rgb = clamp(rgb, vec3(0.0), vec3(1.0)); }
    var linear: vec3<f32>;
    if mapping.color_curve.x > 2.5 {
        linear = rgb;
    } else if mapping.color_curve.x > 1.5 {
        let power = pow(rgb, vec3(32.0 / 2523.0));
        let numerator = max(power - vec3(3424.0 / 4096.0), vec3(0.0));
        let denominator = vec3(2413.0 / 128.0) - (2392.0 / 128.0) * power;
        linear = 125.0 * pow(numerator / denominator, vec3(16384.0 / 2610.0));
    } else if mapping.color_curve.x > 0.5 {
        linear = pow(rgb, vec3(2.2));
    } else {
        linear = select(rgb / 12.92, pow((rgb + 0.055) / 1.055, vec3(2.4)), rgb > vec3(0.04045));
    }
    let converted = vec3(dot(mapping.color_red.xyz, linear), dot(mapping.color_green.xyz, linear), dot(mapping.color_blue.xyz, linear));
    return vec4(converted * mapping.color_curve.y, alpha);
}
