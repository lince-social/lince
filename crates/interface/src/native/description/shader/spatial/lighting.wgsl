#import bevy_pbr::{pbr_types, pbr_functions, mesh_bindings::mesh, mesh_view_bindings::view}

struct LinceLighting {
    parameters: vec4<f32>,
    repeat_opacity: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> lince_lighting: LinceLighting;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var lince_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var lince_sampler: sampler;

fn lince_lit_color(input: LinceVaryings, raw: vec4<f32>) -> vec4<f32> {
    let safe = select(vec4<f32>(0.0), clamp(raw, vec4<f32>(0.0), vec4<f32>(1.0)), all(abs(raw) <= vec4<f32>(1000000.0)));
    let original = safe.rgb * clamp(shader_inputs.effect_strength, 0.0, 1.0);
    if lince_lighting.parameters.w < 0.5 {
        return vec4<f32>(original, 1.0);
    }
    let color = original * textureSample(lince_texture, lince_sampler, input.uv * lince_lighting.repeat_opacity.xy).rgb;
    if lince_lighting.parameters.w < 1.5 {
        return vec4<f32>(color, lince_lighting.repeat_opacity.z);
    }
    var pbr = pbr_types::pbr_input_new();
    pbr.material.base_color = vec4<f32>(color, 1.0);
    pbr.material.perceptual_roughness = lince_lighting.parameters.x;
    pbr.material.metallic = lince_lighting.parameters.y;
    pbr.material.emissive = vec4<f32>(color * lince_lighting.parameters.z, 0.0);
    pbr.world_position = vec4<f32>(input.world_position, 1.0);
    let valid_normal = all(abs(input.normal) <= vec3<f32>(1000000.0)) && length(input.normal) > 0.000001;
    pbr.N = normalize(select(vec3<f32>(0.0, 1.0, 0.0), input.normal, valid_normal));
    pbr.world_normal = pbr.N;
    pbr.is_orthographic = view.clip_from_view[3].w == 1.0;
    pbr.V = pbr_functions::calculate_view(pbr.world_position, pbr.is_orthographic);
    pbr.frag_coord = input.position;
    pbr.flags = mesh[input.instance_index].flags;
    return vec4<f32>(pbr_functions::apply_pbr_lighting(pbr).rgb, lince_lighting.repeat_opacity.z);
}
