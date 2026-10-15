use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_LIGHTS: usize = 8;
pub const MAX_MATERIALS: usize = 64;
pub const MAX_SHADOW_LIGHTS: usize = 2;
pub const MAX_TEXTURES: usize = 16;
pub const MAX_TEXTURE_SIDE: u32 = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Texture {
    #[default]
    Plain,
    Grain,
    Tiles,
    Brushed,
    Asset {
        id: String,
    },
}

impl Texture {
    pub fn valid(&self) -> bool {
        match self {
            Self::Asset { id } => id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub lit: bool,
    pub color: [f32; 4],
    pub texture: Texture,
    pub repeat: [f32; 2],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: f32,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            lit: false,
            color: [0.4, 0.4, 0.45, 1.0],
            texture: Texture::Plain,
            repeat: [1.0; 2],
            roughness: 0.65,
            metallic: 0.0,
            emissive: 0.0,
            cast_shadows: true,
            receive_shadows: true,
        }
    }
}

impl Material {
    pub fn valid(&self) -> bool {
        self.color
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.texture.valid()
            && self
                .repeat
                .iter()
                .all(|value| value.is_finite() && (0.25..=64.0).contains(value))
            && self.roughness.is_finite()
            && (0.089..=1.0).contains(&self.roughness)
            && self.metallic.is_finite()
            && (0.0..=1.0).contains(&self.metallic)
            && self.emissive.is_finite()
            && (0.0..=32.0).contains(&self.emissive)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ShadowQuality {
    #[default]
    Disabled,
    Low,
    Medium,
    High,
}

impl ShadowQuality {
    pub fn resolution(self) -> Option<u32> {
        match self {
            Self::Disabled => None,
            Self::Low => Some(256),
            Self::Medium => Some(512),
            Self::High => Some(1024),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LightDrive {
    #[default]
    Constant,
    Pending,
    Completed,
    Progress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Light {
    pub enabled: bool,
    pub position: [f64; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub shadows: ShadowQuality,
    pub drive: LightDrive,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            enabled: true,
            position: [0.0, 120.0, 0.0],
            color: [1.0; 3],
            intensity: 8000.0,
            range: 1000.0,
            shadows: ShadowQuality::Disabled,
            drive: LightDrive::Constant,
        }
    }
}

impl Light {
    pub fn valid(&self) -> bool {
        self.position
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 100_000.0)
            && self
                .color
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.intensity.is_finite()
            && (0.0..=100_000.0).contains(&self.intensity)
            && self.range.is_finite()
            && (1.0..=100_000.0).contains(&self.range)
    }
}
