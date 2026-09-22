use bevy::{
    prelude::*,
    winit::{UpdateMode, WinitSettings},
};
use std::time::Duration;

pub const PAPER: Color = Color::srgb(18.0 / 255.0, 18.0 / 255.0, 20.0 / 255.0);
pub const INK: Color = Color::srgb(248.0 / 255.0, 250.0 / 255.0, 252.0 / 255.0);
pub const PURPLE: Color = Color::srgb(99.0 / 255.0, 102.0 / 255.0, 241.0 / 255.0);

#[derive(Resource)]
pub struct Typography(pub Handle<Font>);

impl FromWorld for Typography {
    fn from_world(world: &mut World) -> Self {
        let symbols = world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../assets/fonts/NotoSansSymbols2/NotoSansSymbols2-Regular.ttf")
                .to_vec(),
        ));
        let arrows = world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../assets/fonts/DejaVuSans/DejaVuSans.ttf").to_vec(),
        ));
        world.insert_resource(SymbolFont([symbols, arrows]));
        Self(world.resource_mut::<Assets<Font>>().add(Font::from_bytes(
            include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        )))
    }
}

impl Typography {
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

#[derive(Resource)]
struct SymbolFont([Handle<Font>; 2]);

fn symbol_fallbacks(
    mut fonts: Option<ResMut<bevy::text::FontCx>>,
    symbols: Res<SymbolFont>,
    assets: Res<Assets<Font>>,
) {
    if symbols.0.iter().any(|font| assets.get(font).is_none()) {
        return;
    }
    let Some(fonts) = fonts.as_mut() else { return };
    configure_fallbacks(fonts);
}

fn configure_fallbacks(fonts: &mut bevy::text::FontCx) {
    for family in ["Noto Sans Symbols2", "DejaVu Sans"] {
        let Some(id) = fonts.collection.family_id(family) else {
            continue;
        };
        for tag in [*b"Latn", *b"Zyyy", *b"Zinh", *b"Hani"] {
            let script = fontique::Script::from_bytes(tag);
            if !fonts
                .collection
                .fallback_families(script)
                .any(|family| family == id)
            {
                fonts.collection.append_fallbacks(script, [id].into_iter());
            }
        }
    }
}

pub struct ThemePlugin;
impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typography>()
            .add_systems(
                PostUpdate,
                symbol_fallbacks
                    .after(bevy::text::load_font_assets_into_font_collection)
                    .before(bevy::text::EditableTextSystems)
                    .before(bevy::ui::UiSystems::Content),
            )
            .add_plugins(crate::token_style::TokenStylePlugin);
    }
}

pub fn idle_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(Duration::MAX),
        unfocused_mode: UpdateMode::reactive_low_power(Duration::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_symbols_render_without_system_fonts_and_survive_collection_reload() {
        let mut fonts = bevy::text::FontCx::default();
        for _ in 0..2 {
            fonts.collection = fontique::Collection::new(fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            });
            for bytes in [
                include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf").as_slice(),
                include_bytes!("../../../assets/fonts/DejaVuSans/DejaVuSans.ttf").as_slice(),
                include_bytes!(
                    "../../../assets/fonts/NotoSansSymbols2/NotoSansSymbols2-Regular.ttf"
                )
                .as_slice(),
            ] {
                let font = Font::from_bytes(bytes.to_vec());
                fonts.collection.register_fonts(font.data.clone(), None);
            }
            fonts.set_sans_serif_family("Lato").unwrap();
            configure_fallbacks(&mut fonts);
            configure_fallbacks(&mut fonts);
            let mut layout = bevy::text::LayoutCx::default();
            for symbol in [
                "▾", "▴", "▦", "✓", "○", "☐", "☑", "←", "→", "↑", "↓", "↕", "⌃", "⌄", "│", "─",
            ] {
                let mut text = bevy::text::EditableText::new(symbol);
                let shaped = text.editor.layout(&mut fonts.context, &mut layout.0);
                let mut count = 0;
                for line in shaped.lines() {
                    for run in line.runs() {
                        for cluster in run.clusters() {
                            for glyph in cluster.glyphs() {
                                assert_ne!(glyph.id, 0, "Missing symbol {symbol}");
                                count += 1;
                            }
                        }
                    }
                }
                assert!(count > 0, "No glyph for {symbol}");
            }
        }
    }
}
