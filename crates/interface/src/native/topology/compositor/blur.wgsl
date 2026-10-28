#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> parameters: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var backdrop: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var backdrop_sampler: sampler;

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let dimensions = vec2<f32>(textureDimensions(backdrop));
    let low = (view.viewport.xy + vec2(0.5)) / dimensions;
    let high = min((view.viewport.xy + view.viewport.zw - vec2(0.5)) / dimensions, vec2(1.0));
    let point = input.position.xy / dimensions;
    let logical = input.uv * parameters.zw;
    let density = max(max(length(dpdx(logical)), length(dpdy(logical))), 1e-6);
    let radius = min(parameters.x / density, max(dimensions.x, dimensions.y));
    let original = textureSampleLevel(backdrop, backdrop_sampler, clamp(point, low, high), 0.0);
    var color = vec4(0.0);
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let weight = select(1.0, 2.0, x == 0) * select(1.0, 2.0, y == 0);
            let offset = vec2(f32(x), f32(y)) * radius / dimensions;
            color += textureSampleLevel(backdrop, backdrop_sampler, clamp(point + offset, low, high), 0.0) * weight;
        }
    }
    return mix(original, color / 16.0, clamp(parameters.y, 0.0, 1.0));
}
