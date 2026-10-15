use super::*;
use crate::native::tokens::{SandStyleKind, ThemeSettings, Token, TokenValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct Palette {
    pub fur: [u8; 4],
    pub light: [u8; 4],
    pub accent: [u8; 4],
}

impl Default for Palette {
    fn default() -> Self {
        let settings = ThemeSettings::default();
        let read = |token| match settings
            .resolve(token, Some(SandStyleKind::Fiote), &default())
            .0
        {
            TokenValue::Color(color) => color,
            TokenValue::Number(_) => unreachable!(),
        };
        Self {
            fur: read(Token::SandBackground),
            light: read(Token::SandInk),
            accent: read(Token::Accent),
        }
    }
}

impl Palette {
    pub fn resolve(world: &World, entity: Entity) -> Self {
        let read = |token| match {
            let (value, source) = crate::native::token_style::resolve(world, entity, token);
            if crate::native::token_style::inherited_kind(world, entity).is_none()
                && matches!(source, "Theme" | "Default" | "All Sands")
            {
                world
                    .resource::<ThemeSettings>()
                    .resolve(token, Some(SandStyleKind::Fiote), &default())
                    .0
            } else {
                value
            }
        } {
            TokenValue::Color(color) => color,
            TokenValue::Number(_) => unreachable!(),
        };
        Self {
            fur: read(Token::SandBackground),
            light: read(Token::SandInk),
            accent: read(Token::Accent),
        }
    }

    pub fn svg(self) -> String {
        let mapping = [
            ("#bbc1c2", mix(self.fur, self.light, 0.08)),
            ("#737e86", self.fur),
            ("#7e8990", mix(self.fur, self.light, 0.12)),
            ("#849097", self.fur),
            ("#35434d", mix(self.light, self.fur, 0.25)),
            ("#26333e", mix(self.light, self.fur, 0.35)),
            ("#cbb3aa", mix(self.light, self.fur, 0.30)),
            ("#dddcd0", mix(self.light, self.fur, 0.12)),
            ("#faf0db", self.light),
            ("#f5eddb", self.light),
            ("#d4cebf", mix(self.light, self.fur, 0.15)),
            ("#d7dbd6", mix(self.light, self.fur, 0.15)),
            ("#748087", mix(self.light, self.fur, 0.45)),
            ("#59656b", mix(self.light, self.fur, 0.45)),
            ("#536168", mix(self.light, self.fur, 0.45)),
            ("#758087", mix(self.light, self.fur, 0.45)),
            ("#cbd59c", mix(self.light, self.fur, 0.12)),
            ("#4b3e40", mix(self.light, self.fur, 0.70)),
            ("#fff", self.light),
        ];
        let mut svg = String::from_utf8(SVG.to_vec()).expect("packaged Fiote SVG");
        for (index, (original, _)) in mapping.iter().enumerate() {
            svg = svg.replace(original, &format!("FIOTE_COLOR_{index}_"));
        }
        for (index, (_, value)) in mapping.iter().enumerate() {
            svg = svg.replace(
                &format!("FIOTE_COLOR_{index}_"),
                &format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    value[0], value[1], value[2], value[3]
                ),
            );
        }
        svg = svg.replace(
            "<defs>",
            &format!(
                "<style>.fiote-pupil{{fill:#{:02x}{:02x}{:02x}{:02x}}}</style><defs>",
                self.fur[0], self.fur[1], self.fur[2], self.fur[3]
            ),
        );
        svg
    }
}

pub(super) fn mix(first: [u8; 4], second: [u8; 4], amount: f32) -> [u8; 4] {
    std::array::from_fn(|index| {
        (f32::from(first[index]) * (1.0 - amount) + f32::from(second[index]) * amount).round() as u8
    })
}

#[derive(Clone, Copy)]
pub(super) enum Role {
    Fur,
    Light,
    Detail,
    Motion,
}

impl Role {
    pub fn from_name(name: &str) -> Self {
        if name.starts_with("Motion:") {
            Self::Motion
        } else if name == "Silver grey coat" {
            Self::Fur
        } else if name == "Black ear tufts and tail tip" {
            Self::Detail
        } else {
            Self::Light
        }
    }

    pub fn apply(self, material: &mut StandardMaterial, palette: Palette, motion: f32) {
        let rgba = match self {
            Self::Fur => palette.fur,
            Self::Light => mix(palette.light, palette.fur, 0.12),
            Self::Detail => mix(palette.light, palette.fur, 0.62),
            Self::Motion => mix(palette.fur, palette.accent, motion * 0.22),
        };
        let color = Color::srgba_u8(rgba[0], rgba[1], rgba[2], rgba[3]);
        let mut emission = if matches!(self, Self::Motion) {
            Color::srgba_u8(palette.accent[0], palette.accent[1], palette.accent[2], 255)
                .to_linear()
                * (motion * 0.10)
        } else {
            LinearRgba::BLACK
        };
        emission.alpha = 0.0;
        if material.base_color != color {
            material.base_color = color;
        }
        if material.emissive != emission {
            material.emissive = emission;
        }
        let alpha = if rgba[3] == 255 {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        };
        if material.alpha_mode != alpha {
            material.alpha_mode = alpha;
        }
    }
}

pub(super) struct Binding {
    pub material: Handle<StandardMaterial>,
    pub role: Role,
    pub joint: Option<Entity>,
    pub previous: Option<Transform>,
    pub motion: f32,
}

pub(super) fn motion_strength(
    previous: Transform,
    current: Transform,
    delta: f32,
    previous_strength: f32,
) -> f32 {
    if !delta.is_finite() || delta <= 0.0 {
        return previous_strength;
    }
    let speed = (previous.translation.distance(current.translation) * 4.0
        + previous.rotation.angle_between(current.rotation) * 0.25)
        / delta;
    let target = speed.clamp(0.0, 1.0);
    let amount = 1.0 - (-delta.min(0.1) * 8.0).exp();
    let next = previous_strength + (target - previous_strength) * amount;
    if next < 0.005 { 0.0 } else { next }
}

pub(super) fn motion_effects(world: &mut World) {
    let delta = world
        .get_resource::<Time>()
        .map_or(1.0 / 30.0, Time::delta_secs);
    let mut scenes = std::mem::take(&mut world.resource_mut::<Character>().scenes);
    for scene in &mut scenes {
        if !scene.running {
            continue;
        }
        for binding in &mut scene.materials {
            let Some(joint) = binding.joint else { continue };
            let Some(current) = world
                .get::<GlobalTransform>(joint)
                .map(GlobalTransform::compute_transform)
            else {
                continue;
            };
            if let Some(previous) = binding.previous {
                binding.motion = motion_strength(previous, current, delta, binding.motion);
            }
            binding.previous = Some(current);
            if let Some(mut material) = world
                .resource_mut::<Assets<StandardMaterial>>()
                .get_mut(&binding.material)
            {
                binding
                    .role
                    .apply(&mut material, scene.palette, binding.motion);
            }
        }
    }
    world.resource_mut::<Character>().scenes = scenes;
}
