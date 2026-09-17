#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> ink: vec4<f32>;
@group(1) @binding(1) var<uniform> geometry: vec4<f32>;
@group(1) @binding(2) var<uniform> viewport: vec4<f32>;
@group(1) @binding(3) var<uniform> eye: vec4<f32>;
@group(1) @binding(4) var<uniform> rotation: mat4x4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var position = in.uv * viewport.xy - geometry.xy;
    var visible = 1.0;
    if eye.w > 0.0 {
        let screen = (in.uv * 2.0 - 1.0) * vec2(viewport.x / viewport.y, -1.0) * viewport.w;
        let ray = (rotation * vec4(screen, -1.0, 0.0)).xyz;
        let denominator = select(-0.000001, ray.y, abs(ray.y) > 0.000001);
        let travel = -eye.y / denominator;
        position = eye.xz + ray.xz * clamp(travel, 0.0, 1000000.0);
        visible = select(0.0, 1.0, travel > 0.0 && travel < 1000000.0 && abs(ray.y) > 0.000001);
    }
    let cell = abs(position - floor(position / geometry.z + 0.5) * geometry.z);
    let reach = geometry.z * 0.5 * geometry.w;
    let horizontal = length(vec2(max(cell.x - reach, 0.0), cell.y));
    let vertical = length(vec2(cell.x, max(cell.y - reach, 0.0)));
    let feather = max(fwidth(position.x), fwidth(position.y)) * 0.5;
    let thickness = select(viewport.z, viewport.z * max(feather * 2.0, 1.0), eye.w > 0.0);
    let distance = min(horizontal, vertical) - thickness * 0.5;
    let coverage = 1.0 - smoothstep(-feather, feather, distance);
    let fade = select(1.0, 1.0 - smoothstep(geometry.z * 0.125, geometry.z * 0.5, feather), eye.w > 0.0);
    return vec4(ink.rgb, ink.a * coverage * visible * fade);
}
