//! Validates spatial stage bounds and compiles their callback adapters.
// non-lean-yet

use super::{MESH_TYPES, MeshEntry, Mode};
use crate::{
    HEADER, compile,
    compiler::{compile_with_budget, compile_with_callback_budget},
};

fn check_source(source: &str) -> Result<(), String> {
    if source.len() > 65_536 || source.lines().count() > 2048 {
        Err("WGSL is limited to 64 KB and 2048 lines per stage.".into())
    } else {
        Ok(())
    }
}

pub fn mesh_entry(source: &str) -> Result<MeshEntry, String> {
    check_source(source)?;
    let text = format!("{HEADER}{MESH_TYPES}{source}");
    let module =
        naga::front::wgsl::parse_str(&text).map_err(|error| error.emit_to_string(&text))?;
    let point = module
        .functions
        .iter()
        .any(|(_, function)| function.name.as_deref() == Some("shade_mesh"));
    let uv = module
        .functions
        .iter()
        .any(|(_, function)| function.name.as_deref() == Some("shade"));
    if point && uv {
        return Err("Define shade_mesh or shade for a mesh program, rather than both.".into());
    }
    Ok(if point {
        MeshEntry::Point
    } else {
        MeshEntry::Uv
    })
}

fn adapter(source: &str, types: &str, body: &str, budget: usize) -> Result<(), String> {
    check_source(source)?;
    compile_with_budget(&format!("{types}\n{source}\nfn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> {{ {body} }}"), budget).map(|_| ())
}

impl Mode {
    pub fn validate(&self) -> Result<(), String> {
        if !self.bounded() {
            return Err("Use bounded stage sources and finite spatial settings.".into());
        }
        match self {
            Self::Surface => {}
            Self::Mesh(settings) => {
                if let Some(stage) = &settings.deformation {
                    if !(0.0..=0.5).contains(&stage.max_displacement) {
                        return Err(
                            "Vertex displacement must be between 0 and 0.5 local units.".into()
                        );
                    }
                    adapter(
                        &stage.source,
                        MESH_TYPES,
                        "let vertex = deform_vertex(MeshVertex(vec3<f32>(uv - 0.5, 0.0), vec3<f32>(0.0, 0.0, 1.0), uv), inputs); return vec4<f32>(vertex.position, 1.0);",
                        256,
                    )?;
                }
                if let Some(stage) = &settings.geometry {
                    if stage.vertices < 3
                        || stage.vertices > 4096
                        || !stage.vertices.is_multiple_of(3)
                    {
                        return Err(
                            "Generated triangle lists use 3–4096 vertices in multiples of three."
                                .into(),
                        );
                    }
                    adapter(
                        &stage.source,
                        MESH_TYPES,
                        &format!(
                            "let vertex = generate_vertex(0u, {}u, inputs); return vec4<f32>(vertex.position, 1.0);",
                            stage.vertices
                        ),
                        256,
                    )?;
                }
            }
            Self::Volume(settings) => {
                if !(0.0..=8.0).contains(&settings.density) {
                    return Err("Volume density must be between 0 and 8.".into());
                }
            }
        }
        Ok(())
    }

    pub fn validate_program(&self, source: &str) -> Result<(), String> {
        match self {
            Self::Surface => compile(source).map(|_| ()),
            Self::Mesh(_) => match mesh_entry(source)? {
                MeshEntry::Uv => compile(&format!("{MESH_TYPES}{source}")).map(|_| ()),
                MeshEntry::Point => adapter(
                    source,
                    MESH_TYPES,
                    "let point = MeshPoint(vec3<f32>(uv - 0.5, 0.0), vec3<f32>(uv - 0.5, 0.0), vec3<f32>(0.0, 0.0, 1.0), uv); return shade_mesh(point, inputs);",
                    1024,
                ),
            },
            Self::Volume(settings) => {
                let budget = 4096 / settings.quality.steps() as usize - 32;
                check_source(source)?;
                compile_with_callback_budget(
                    &format!("{source}\nfn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> {{ return volume_sample(vec3<f32>(uv - 0.5, 0.0), inputs); }}"),
                    budget,
                    Some("volume_sample"),
                ).map(|_| ())
                .map_err(|error| {
                    format!(
                        "Volume field at {} samples: {error}",
                        settings.quality.steps()
                    )
                })
            }
        }
    }
}
