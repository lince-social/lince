// non-lean-yet

//! Shared UI palette, typography, and bundled fallback fonts.

use crate::ithkuil::IthkuilPlugin;
use bevy::{
    prelude::*,
    text::{
        EditableTextSystems, FontCx, detect_text_needs_rerender,
        load_font_assets_into_font_collection,
    },
    ui::UiSystems,
};
use fontique::{Blob, Collection, Script};
use std::{iter::once, sync::Arc};

mod cjk;

/// Default dark background.
pub const PAPER: Color = Color::srgb(18.0 / 255.0, 18.0 / 255.0, 20.0 / 255.0);
/// Default light foreground.
pub const INK: Color = Color::srgb(248.0 / 255.0, 250.0 / 255.0, 252.0 / 255.0);
/// Default purple accent.
pub const PURPLE: Color = Color::srgb(99.0 / 255.0, 102.0 / 255.0, 241.0 / 255.0);

const LATO: &[u8] = include_bytes!("../../../institute/assets/fonts/Lato/Lato-Regular.ttf");
const SYMBOLS: &[u8] =
    include_bytes!("../../../institute/assets/fonts/NotoSansSymbols2/NotoSansSymbols2-Regular.ttf");
const ARROWS: &[u8] = include_bytes!("../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf");

/// Handle for the primary bundled UI font.
#[derive(Resource)]
pub struct Typography(pub Handle<Font>);

impl FromWorld for Typography {
    fn from_world(world: &mut World) -> Self {
        let mut assets = world.resource_mut::<Assets<Font>>();
        let fallbacks = [SYMBOLS, ARROWS].map(|bytes| assets.add(bundled_font(bytes)));
        world.insert_resource(SymbolFont(fallbacks));
        Self(world.resource_mut::<Assets<Font>>().add(bundled_font(LATO)))
    }
}

impl Typography {
    /// Returns a text style using the primary font at the requested pixel size.
    pub fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

pub(crate) fn bundled_font(bytes: &'static [u8]) -> Font {
    Font {
        data: Blob::new(Arc::new(bytes)),
        alias: String::new(),
    }
}

#[derive(Resource)]
struct SymbolFont([Handle<Font>; 2]);

fn symbol_fallbacks(
    fonts: Option<ResMut<FontCx>>,
    symbols: Res<SymbolFont>,
    assets: Res<Assets<Font>>,
) {
    if symbols.0.iter().any(|font| assets.get(font).is_none()) {
        return;
    }
    if let Some(mut fonts) = fonts {
        configure_fallbacks(&mut fonts);
    }
}

fn configure_fallbacks(fonts: &mut FontCx) {
    for family in ["Noto Sans Symbols2", "DejaVu Sans"] {
        register_fallbacks(
            &mut fonts.collection,
            family,
            [*b"Latn", *b"Zyyy", *b"Zinh", *b"Hani"],
        );
    }
}

fn register_fallbacks(
    collection: &mut Collection,
    family: &str,
    tags: impl IntoIterator<Item = [u8; 4]>,
) -> bool {
    let Some(id) = collection.family_id(family) else {
        return false;
    };
    let mut changed = false;
    for tag in tags {
        let script = Script::from_bytes(tag);
        if !collection
            .fallback_families(script)
            .any(|family| family == id)
        {
            collection.append_fallbacks(script, once(id));
            changed = true;
        }
    }
    changed
}

/// Installs bundled typography and script fallbacks before UI layout.
pub struct TypographyPlugin;
impl Plugin for TypographyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typography>()
            .add_plugins(IthkuilPlugin)
            .init_resource::<cjk::Fallback>()
            .add_systems(
                PostUpdate,
                cjk::load.before(load_font_assets_into_font_collection),
            )
            .add_systems(
                PostUpdate,
                (symbol_fallbacks, cjk::configure)
                    .chain()
                    .after(load_font_assets_into_font_collection)
                    .before(detect_text_needs_rerender)
                    .before(EditableTextSystems)
                    .before(UiSystems::Content),
            )
            .add_systems(Last, cjk::load);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::text::{EditableText, LayoutCx};
    use fontique::CollectionOptions;

    #[test]
    fn bundled_symbols_render_without_system_fonts_and_survive_collection_reload() {
        let mut fonts = FontCx::default();
        for _ in 0..2 {
            fonts.collection = Collection::new(CollectionOptions {
                shared: false,
                system_fonts: false,
            });
            for bytes in [LATO, ARROWS, SYMBOLS] {
                fonts
                    .collection
                    .register_fonts(bundled_font(bytes).data, None);
            }
            fonts.set_sans_serif_family("Lato").unwrap();
            configure_fallbacks(&mut fonts);
            configure_fallbacks(&mut fonts);
            let mut layout = LayoutCx::default();
            for symbol in [
                "▾", "▴", "▦", "✓", "○", "☐", "☑", "←", "→", "↑", "↓", "↕", "⌃", "⌄", "│", "─",
            ] {
                let mut text = EditableText::new(symbol);
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
