#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> background: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> ink: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> pattern: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> phase: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let light = 0.82 + 0.18 * dot(normal, normalize(vec3(-0.4, 1.0, -0.3)));
    let height = in.world_position.y;
    let raised = 0.08 * tanh(height / 100.0);
    var color = background.rgb * light + vec3(raised);
    let position = in.uv + phase.xy + vec2(height * 0.12, height * 0.06);
    let cell = abs(position - floor(position / pattern.x + 0.5) * pattern.x);
    let reach = pattern.x * 0.5 * clamp(pattern.z, 0.0, 1.0);
    let distance = min(length(vec2(max(cell.x - reach, 0.0), cell.y)), length(vec2(cell.x, max(cell.y - reach, 0.0)))) - pattern.y * 0.5;
    let feather = max(max(fwidth(position.x), fwidth(position.y)), 0.01);
    let coverage = (1.0 - smoothstep(-feather, feather, distance)) * select(0.0, 1.0, pattern.z > 0.0);
    color = mix(color, ink.rgb, coverage);
    let contour_distance = abs(height - floor(height / pattern.w + 0.5) * pattern.w);
    let contour = (1.0 - smoothstep(0.2, max(fwidth(height), 0.6), contour_distance)) * min(length(normal.xz) * 2.0, 0.45);
    return vec4(clamp(color + vec3(contour * 0.12), vec3(0.0), vec3(1.0)), 1.0);
}
