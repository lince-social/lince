//! Spatial shader stage settings and wire types.
// non-lean-yet

use serde::{Deserialize, Serialize};

#[cfg(feature = "compiler")]
mod admission;
#[cfg(feature = "compiler")]
pub use admission::mesh_entry;

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
}

#[cfg(all(test, feature = "compiler"))]
mod tests;
