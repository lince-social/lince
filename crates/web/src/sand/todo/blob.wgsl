struct BlobUniforms {
    viewport_time: vec4<f32>,
    current_target: vec4<f32>,
    params: vec4<f32>,
    color0: vec4<f32>,
    color1: vec4<f32>,
    color2: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: BlobUniforms;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    let position = positions[vertex_index];
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return out;
}

fn metaball(point: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    let offset = point - center;
    return (radius * radius) / max(dot(offset, offset), 1.0);
}

fn soft_rgb(time: f32, energy: f32, color0: vec3<f32>, color1: vec3<f32>, color2: vec3<f32>) -> vec3<f32> {
    let wave0 = 0.5 + 0.5 * sin(time * (1.4 + energy * 2.2));
    let wave1 = 0.5 + 0.5 * sin(time * (1.1 + energy * 1.7) + 2.094);
    let wave2 = 0.5 + 0.5 * sin(time * (1.7 + energy * 2.6) + 4.188);
    return normalize(color0 * wave0 + color1 * wave1 + color2 * wave2 + vec3<f32>(0.05, 0.05, 0.05));
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let size = max(u.viewport_time.xy, vec2<f32>(1.0, 1.0));
    let time = u.viewport_time.z;
    let visible = clamp(u.viewport_time.w, 0.0, 1.0);
    let point = uv * size;

    let current = u.current_target.xy;
    let destination = u.current_target.zw;
    let viscosity = clamp(u.params.x, 0.0, 1.0);
    let energy = clamp(u.params.y, 0.0, 1.0);
    let completion = clamp(u.params.w, 0.0, 1.0);

    let pull = destination - current;
    let pull_len = length(pull);
    var direction = vec2<f32>(1.0, 0.0);
    if pull_len > 0.001 {
        direction = pull / pull_len;
    }
    let normal = vec2<f32>(-direction.y, direction.x);
    let max_dimension = max(size.x, size.y);

    let cursor_radius = 18.0 + energy * 18.0;
    let burst_radius = max_dimension * (0.16 + completion * 0.62);
    let radius = mix(cursor_radius, burst_radius, completion);
    let wobble = (1.0 - viscosity) * energy;
    let orbit = 7.0 + energy * 28.0 + completion * max_dimension * 0.22;

    let center0 = current + direction * sin(time * 2.1) * orbit * wobble;
    let center1 = current - direction * (6.0 + energy * 12.0) + normal * sin(time * 3.0) * orbit * 0.42 * wobble;
    let center2 = current + direction * (5.0 + energy * 10.0) - normal * cos(time * 2.7) * orbit * 0.34 * wobble;
    let center3 = current + normal * sin(time * 2.4 + 1.7) * orbit * (0.28 + energy * 0.22);

    var field = 0.0;
    field += metaball(point, center0, radius);
    field += metaball(point, center1, radius * (0.62 + energy * 0.18));
    field += metaball(point, center2, radius * (0.56 + energy * 0.2));
    field += metaball(point, center3, radius * (0.38 + energy * 0.25));

    let ripple = sin((point.x * 0.045 + point.y * 0.032) + time * (3.0 + energy * 8.0));
    let threshold = 0.86 - energy * 0.26 - completion * 0.2;
    let softness = mix(0.22, 0.07, viscosity);
    let body = smoothstep(threshold - softness, threshold + softness, field + ripple * energy * 0.055);
    let aura = smoothstep(0.035, threshold, field) * (1.0 - body * 0.18);
    let rim = smoothstep(threshold - softness * 2.5, threshold, field) * (1.0 - smoothstep(threshold, threshold + softness * 4.0, field));

    let c0 = u.color0.rgb;
    let c1 = u.color1.rgb;
    let c2 = u.color2.rgb;
    let rgb = soft_rgb(time, energy, c0, c1, c2);
    let vein = 0.5 + 0.5 * sin(field * 14.0 + point.x * 0.07 - point.y * 0.04 + time * (2.0 + energy * 7.0));
    let liquid = mix(rgb * 0.62, c1, body * 0.35 + vein * energy * 0.2);
    let glow = rgb * (0.28 + aura * (0.8 + energy * 0.8));
    let edge = c2 * rim * (1.2 + energy);

    let alpha = visible * clamp(body * 0.96 + aura * 0.55 + rim * 0.65, 0.0, 1.0);
    let color = liquid * body + glow * aura + edge;
    return vec4<f32>(color * alpha, alpha);
}
