fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> {
    if inputs.quantity_present == 0u || inputs.task_state == 3u {
        return vec4<f32>(0.0);
    }
    if inputs.task_state == 0u && inputs.progress_present == 0u {
        return vec4<f32>(0.0);
    }
    let dirt = select(1.0, 1.0 - inputs.progress, inputs.progress_present != 0u);
    let wobble = sin(uv.x * 23.0) * sin(uv.y * 19.0) * 0.035;
    let a = 1.0 - smoothstep(0.12, 0.24, length((uv - vec2<f32>(0.28, 0.32)) * vec2<f32>(1.0, 1.35)) + wobble);
    let b = 1.0 - smoothstep(0.14, 0.29, length((uv - vec2<f32>(0.68, 0.64)) * vec2<f32>(1.15, 1.0)) - wobble);
    let c = 1.0 - smoothstep(0.04, 0.14, length(uv - vec2<f32>(0.5, 0.5)) + wobble);
    let edge = smoothstep(0.0, 0.05, min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y)));
    return vec4<f32>(vec3<f32>(0.025, 0.018, 0.012), clamp(max(max(a, b), c) * dirt * edge, 0.0, 1.0));
}
