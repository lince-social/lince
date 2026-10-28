#import bevy_pbr::forward_io::VertexOutput

struct Parameters {
    bounds: vec4<f32>,
    style: vec4<f32>,
    edges: array<vec4<f32>, 64>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> parameters: Parameters;

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let point = mix(parameters.bounds.xy, parameters.bounds.zw, input.uv);
    var inside = false;
    var distance = 1e20;
    for (var index = 0u; index < u32(parameters.style.z); index += 1u) {
        let edge = parameters.edges[index];
        let start = edge.xy;
        let end = edge.zw;
        let delta = end - start;
        let fraction = clamp(dot(point - start, delta) / max(dot(delta, delta), 1e-12), 0.0, 1.0);
        distance = min(distance, length(point - start - fraction * delta));
        if ((start.y > point.y) != (end.y > point.y)) {
            let crossing = start.x + (point.y - start.y) * delta.x / delta.y;
            if (point.x < crossing) { inside = !inside; }
        }
    }
    if (inside) { discard; }
    let radius = max(parameters.style.x, 0.001);
    let feather = 1.0 - smoothstep(radius * 2.5, radius * 3.0, distance);
    let opacity = exp(-0.5 * distance * distance / (radius * radius)) * feather * 0.24 * parameters.style.y;
    return vec4(0.0, 0.0, 0.0, opacity);
}
