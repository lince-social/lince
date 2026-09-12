#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> ink: vec4<f32>;
@group(1) @binding(1) var<uniform> geometry: vec4<f32>;
@group(1) @binding(2) var<uniform> viewport: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let position = in.uv * viewport.xy - geometry.xy;
    let cell = abs(position - floor(position / geometry.z + 0.5) * geometry.z);
    let reach = geometry.z * 0.5 * geometry.w;
    let horizontal = length(vec2(max(cell.x - reach, 0.0), cell.y));
    let vertical = length(vec2(cell.x, max(cell.y - reach, 0.0)));
    let distance = min(horizontal, vertical) - viewport.z * 0.5;
    let feather = max(fwidth(position.x), fwidth(position.y)) * 0.5;
    let coverage = 1.0 - smoothstep(-feather, feather, distance);
    return vec4(ink.rgb, ink.a * coverage);
}
