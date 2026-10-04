use super::*;
use crate::tokens::{SandStyleKind, ThemeSettings, Token, TokenOverrides};

#[derive(Clone, PartialEq)]
pub(super) struct Palette {
    pub face: Color,
    pub ink: Color,
    pub surface: Color,
    pub track: Color,
    pub event: Color,
    pub overlap: Color,
    pub present: Color,
    pub muted: Color,
    pub border: Color,
    pub border_width: f32,
    pub width: f32,
    pub font: f32,
    pub gap: f32,
}

impl Palette {
    pub fn resolve(world: &World, owner: Entity) -> Self {
        let value = |token| {
            if world.contains_resource::<ThemeSettings>() {
                crate::token_style::resolve(world, owner, token).0
            } else {
                ThemeSettings::default()
                    .resolve(
                        token,
                        Some(SandStyleKind::TimeCastle),
                        &TokenOverrides::default(),
                    )
                    .0
            }
        };
        Self {
            face: value(Token::SandBackground).color(),
            ink: value(Token::SandInk).color(),
            surface: value(Token::Surface).color(),
            track: value(Token::ClockTrack).color(),
            event: value(Token::ClockEvent).color(),
            overlap: value(Token::ClockOverlap).color(),
            present: value(Token::ClockPresent).color(),
            muted: value(Token::ClockMuted).color(),
            border: value(Token::SandBorder).color(),
            border_width: value(Token::BorderWidth).number(),
            width: value(Token::ClockRangeWidth).number(),
            font: value(Token::FontSize).number(),
            gap: value(Token::ClockLabelGap).number(),
        }
    }

    pub fn event(&self, lane: usize, selected: bool) -> Color {
        if selected {
            self.present
        } else if lane > 0 {
            self.overlap.mix(&self.event, (lane % 4) as f32 * 0.18)
        } else {
            self.event
        }
    }
}

pub(super) fn css(color: Color) -> String {
    let color = color.to_srgba();
    let bytes = [color.red, color.green, color.blue, color.alpha]
        .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8);
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

pub(super) fn rgba(color: Color) -> [f32; 4] {
    let color = color.to_srgba();
    [color.red, color.green, color.blue, color.alpha]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{ColorScheme, TokenValue};

    #[test]
    fn clock_palette_resolves_global_type_and_local_tokens_and_inherits_after_reset() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        let owner = world.spawn(TimeSettings(Settings::default())).id();
        let other = world.spawn(TimeSettings(Settings::default())).id();
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::ClockEvent, TokenValue::Color([10, 20, 30, 255]));
        assert_eq!(
            Palette::resolve(&world, owner).event,
            Color::srgb_u8(10, 20, 30)
        );
        world
            .resource_mut::<ThemeSettings>()
            .kinds
            .entry(SandStyleKind::TimeCastle)
            .or_default()
            .set(Token::ClockEvent, TokenValue::Color([30, 40, 50, 255]));
        let mut own = TokenOverrides::default();
        own.set(Token::ClockEvent, TokenValue::Color([80, 90, 100, 255]));
        world.entity_mut(owner).insert(own);
        assert_eq!(
            Palette::resolve(&world, owner).event,
            Color::srgb_u8(80, 90, 100)
        );
        assert_eq!(
            Palette::resolve(&world, other).event,
            Color::srgb_u8(30, 40, 50)
        );
        world.entity_mut(owner).remove::<TokenOverrides>();
        assert_eq!(
            Palette::resolve(&world, owner).event,
            Palette::resolve(&world, other).event
        );
        let dark = Palette::resolve(&world, owner);
        world.resource_mut::<ThemeSettings>().scheme = ColorScheme::ComfyPink;
        let pink = Palette::resolve(&world, owner);
        assert_ne!(pink.face, dark.face);
        assert_ne!(pink.track, dark.track);
        assert_ne!(pink.ink, dark.ink);
    }
}
