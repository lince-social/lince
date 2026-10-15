use super::{HEADER, compile, compile_with_budget, compile_with_callback_budget};
use serde::{Deserialize, Serialize};

pub const MESH_TYPES: &str = "struct MeshVertex { position: vec3<f32>, normal: vec3<f32>, uv: vec2<f32> }; struct MeshPoint { local_position: vec3<f32>, world_position: vec3<f32>, normal: vec3<f32>, uv: vec2<f32> };\n";
pub const MESH_SHADE: &str = "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let color = vec3<f32>(0.15, 0.4, 0.8) + abs(point.normal) * 0.2; return vec4<f32>(color, 1.0); }";
pub const DEFORM_SPHERE: &str = "fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex { let amount = clamp(inputs.quantity * f32(inputs.quantity_present) * 0.02, -0.2, 0.2); let wave = 0.04 * sin(vertex.position.y * 12.0 + inputs.time * 2.0) * f32(inputs.motion_allowed); return MeshVertex(vertex.position + vertex.normal * (amount + wave), vertex.normal, vertex.uv); }";
pub const GENERATED_TRIANGLE: &str = "fn generate_vertex(index: u32, vertex_count: u32, inputs: ShaderInputs) -> MeshVertex { let slot = index % 3u; let x = select(-0.4, 0.4, slot == 1u); let y = select(-0.4, 0.4, slot == 2u); let height = clamp(inputs.quantity * f32(inputs.quantity_present) * 0.02, -0.4, 0.4); return MeshVertex(vec3<f32>(x, y, height), vec3<f32>(0.0, 0.0, 1.0), vec2<f32>(x, y) + 0.5); }";
pub const GENERATED_SPHERE: &str = "fn generate_vertex(index: u32, vertex_count: u32, inputs: ShaderInputs) -> MeshVertex { let cell = index / 6u; let corner = index % 6u; let u = f32(cell % 32u + u32(corner == 1u || corner == 2u || corner == 4u)) / 32.0; let v = f32(cell / 32u + u32(corner == 2u || corner == 4u || corner == 5u)) / 20.0; let longitude = u * 6.2831853; let latitude = v * 3.1415927; let normal = vec3<f32>(sin(latitude) * cos(longitude), cos(latitude), sin(latitude) * sin(longitude)); return MeshVertex(normal * 0.5, normal, vec2<f32>(u, v)); }";
pub const VOLUME_CLOUD: &str = "const CLOUD_COLOR: vec3<f32> = vec3<f32>(0.2, 0.65, 1.0); fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(CLOUD_COLOR, clamp((0.2 + min(abs(inputs.quantity), 10.0) * 0.02 - length(position)) * 6.0, 0.0, 1.0)); }";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mode {
    #[default]
    Surface,
    Mesh(MeshSettings),
    Volume(VolumeSettings),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Sphere,
    Plane,
    Cuboid,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshSettings {
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub deformation: Option<Deformation>,
    #[serde(default)]
    pub geometry: Option<GeneratedGeometry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deformation {
    pub source: String,
    pub max_displacement: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedGeometry {
    pub source: String,
    pub vertices: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

impl Quality {
    pub fn steps(self) -> u32 {
        match self {
            Self::Low => 16,
            Self::Medium => 32,
            Self::High => 64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeSettings {
    #[serde(default)]
    pub quality: Quality,
    pub density: f32,
}

impl Default for VolumeSettings {
    fn default() -> Self {
        Self {
            quality: Quality::Medium,
            density: 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshEntry {
    Uv,
    Point,
}

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
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        let (deformation, geometry) = match self {
            Self::Mesh(settings) => (
                settings
                    .deformation
                    .as_ref()
                    .map(|stage| stage.source.as_str()),
                settings
                    .geometry
                    .as_ref()
                    .map(|stage| stage.source.as_str()),
            ),
            _ => (None, None),
        };
        deformation.into_iter().chain(geometry)
    }

    pub fn bounded(&self) -> bool {
        self.sources().all(|source| source.len() <= 65_536)
            && match self {
                Self::Surface => true,
                Self::Mesh(settings) => settings
                    .deformation
                    .as_ref()
                    .is_none_or(|stage| stage.max_displacement.is_finite()),
                Self::Volume(settings) => settings.density.is_finite(),
            }
    }

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
                    if stage.vertices < 3 || stage.vertices > 4096 || stage.vertices % 3 != 0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_effect::{Effect, Extension};

    #[test]
    fn spatial_callbacks_validate_types_and_round_trip_settings() {
        let mesh = Mode::Mesh(MeshSettings {
            shape: Shape::Sphere,
            deformation: Some(Deformation {
                source: DEFORM_SPHERE.into(),
                max_displacement: 0.25,
            }),
            geometry: Some(GeneratedGeometry {
                source: GENERATED_TRIANGLE.into(),
                vertices: 3,
            }),
        });
        assert!(mesh.validate().is_ok());
        assert!(mesh.validate_program(MESH_SHADE).is_ok());
        assert!(mesh.validate_program(super::super::CALM).is_ok());
        assert_eq!(mesh_entry(MESH_SHADE).unwrap(), MeshEntry::Point);
        assert_eq!(mesh_entry(super::super::CALM).unwrap(), MeshEntry::Uv);
        assert_eq!(
            serde_json::from_value::<Mode>(serde_json::to_value(&mesh).unwrap()).unwrap(),
            mesh
        );
        assert!(
            mesh.validate_program(
                "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> f32 { return 1.0; }"
            )
            .is_err()
        );
        assert!(
            mesh.validate_program(&format!("{MESH_SHADE}{}", super::super::CALM))
                .is_err()
        );
        let volume = Mode::Volume(VolumeSettings::default());
        assert!(volume.validate().is_ok());
        assert!(volume.validate_program(VOLUME_CLOUD).is_ok());
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let volume = Mode::Volume(VolumeSettings {
                quality,
                ..VolumeSettings::default()
            });
            assert!(
                volume.validate_program(VOLUME_CLOUD).is_ok(),
                "Cloud preset exceeds {quality:?} quality's work budget"
            );
        }
        assert!(volume.validate_program(MESH_SHADE).is_err());
        assert_eq!(
            [
                Quality::Low.steps(),
                Quality::Medium.steps(),
                Quality::High.steps()
            ],
            [16, 32, 64]
        );
    }

    #[test]
    fn geometry_and_deformation_cannot_choose_unbounded_resources_or_work() {
        for vertices in [0, 1, 4, 4096, u32::MAX] {
            let mode = Mode::Mesh(MeshSettings {
                geometry: Some(GeneratedGeometry {
                    source: GENERATED_TRIANGLE.into(),
                    vertices,
                }),
                ..Default::default()
            });
            assert!(mode.validate().is_err());
        }
        for displacement in [-1.0, 0.51, f32::INFINITY, f32::NAN] {
            let mode = Mode::Mesh(MeshSettings {
                deformation: Some(Deformation {
                    source: DEFORM_SPHERE.into(),
                    max_displacement: displacement,
                }),
                ..Default::default()
            });
            assert!(mode.validate().is_err());
        }
        for source in [
            format!("@group(2) @binding(0) var<storage, read_write> arbitrary: array<f32>; {GENERATED_TRIANGLE}"),
            "fn generate_vertex(index: u32, vertex_count: u32, inputs: ShaderInputs) -> MeshVertex { loop {} return MeshVertex(vec3<f32>(0.0), vec3<f32>(1.0), vec2<f32>(0.0)); }".into(),
        ] {
            let mode = Mode::Mesh(MeshSettings { geometry: Some(GeneratedGeometry { source, vertices: 3 }), ..Default::default() });
            assert!(mode.validate().is_err());
        }
        let mode = Mode::Mesh(MeshSettings {
            geometry: Some(GeneratedGeometry {
                source: GENERATED_TRIANGLE.into(),
                vertices: 4095,
            }),
            ..Default::default()
        });
        assert!(mode.validate().is_ok());
        for density in [-1.0, 8.1, f32::NAN] {
            assert!(
                Mode::Volume(VolumeSettings {
                    density,
                    ..Default::default()
                })
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn volume_quality_charges_field_work_for_every_sample() {
        let mut body = String::from("let n0 = position.x;");
        for index in 1..13 {
            body.push_str(&format!(
                "let n{index} = sin(n{}) + position.y * 0.2;",
                index - 1
            ));
        }
        let source = format!(
            "fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> {{ {body} return vec4<f32>(n12); }}"
        );
        let low = Mode::Volume(VolumeSettings {
            quality: Quality::Low,
            ..Default::default()
        });
        let high = Mode::Volume(VolumeSettings {
            quality: Quality::High,
            ..Default::default()
        });
        assert!(low.validate_program(&source).is_ok());
        assert!(
            high.validate_program(&source)
                .unwrap_err()
                .contains("64 samples")
        );
        assert!(high.validate_program("fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.2); }").is_ok());
    }

    #[test]
    fn custom_helpers_cannot_override_renderer_bounds_or_scene_depth() {
        for helper in [
            "fn length(value: vec3<f32>) -> f32 { return 0.0; }",
            "fn min(left: f32, right: f32) -> f32 { return 100000.0; }",
            "fn max(left: f32, right: f32) -> f32 { return 0.0; }",
        ] {
            let mode = Mode::Mesh(MeshSettings {
                deformation: Some(Deformation {
                    source: format!(
                        "{helper} fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex {{ return MeshVertex(vertex.position * 1000.0, vertex.normal, vertex.uv); }}"
                    ),
                    max_displacement: 0.1,
                }),
                ..Default::default()
            });
            assert!(
                mode.validate()
                    .unwrap_err()
                    .contains("reserved for renderer")
            );
        }
        let mode = Mode::Volume(VolumeSettings::default());
        assert!(mode.validate_program(&format!("fn prepass_depth(position: vec4<f32>, index: u32) -> f32 {{ return 0.0; }} {VOLUME_CLOUD}")).unwrap_err().contains("reserved for renderer"));
        assert!(
            mode.validate_program(&format!(
                "fn my_length(value: vec3<f32>) -> f32 {{ return length(value); }} {VOLUME_CLOUD}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn catalog_source_limits_include_geometry_and_deformation() {
        let mut effect = Effect::default();
        effect.mode = Mode::Mesh(MeshSettings {
            deformation: Some(Deformation {
                source: format!("{DEFORM_SPHERE}{}", " ".repeat(60_000)),
                max_displacement: 0.25,
            }),
            geometry: Some(GeneratedGeometry {
                source: format!("{GENERATED_TRIANGLE}{}", " ".repeat(60_000)),
                vertices: 3,
            }),
            ..Default::default()
        });
        assert!(effect.validate().is_ok());
        let mut catalog = Extension::single(1, effect);
        assert!(catalog.validate().is_ok());
        catalog
            .effects
            .get_mut("main")
            .unwrap()
            .effect
            .programs
            .insert(
                "extra".into(),
                format!("{}{}", super::super::CALM, " ".repeat(12_000)),
            );
        assert!(catalog.validate().unwrap_err().contains("128 KiB"));
        assert!(!catalog.bounded());
    }
}
