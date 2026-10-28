use bevy::{
    prelude::*,
    text::{EditableText, FontFeatureTag, FontFeatures, TextSection},
    ui::prelude::AccessibleLabel,
};
use std::sync::Arc;

pub use lince_ithkuil::{Error, Script, render};

#[derive(Component, Clone)]
pub struct ScriptText(pub Arc<Script>);

impl ScriptText {
    pub fn parse(romanization: &str) -> Result<Self, Error> {
        render(romanization).map(Arc::new).map(Self)
    }
}

#[derive(Component)]
struct Rendered {
    script: Arc<Script>,
    original: TextFont,
    applied: TextFont,
    accessible: bool,
}

#[derive(Resource, Default)]
struct ScriptFont(Option<Handle<Font>>);

fn labels<T: TextSection>(
    mut commands: Commands,
    mut assets: ResMut<Assets<Font>>,
    mut native_font: ResMut<ScriptFont>,
    mut texts: Query<
        (
            Entity,
            &mut T,
            &mut TextFont,
            Option<&ScriptText>,
            Option<&Rendered>,
            Option<&AccessibleLabel>,
        ),
        (
            Without<EditableText>,
            Or<(Changed<T>, Changed<TextFont>, Changed<ScriptText>)>,
        ),
    >,
) {
    for (entity, mut text, mut font, explicit, previous, accessible) in &mut texts {
        let value = text.get_text();
        let script = explicit
            .map(|explicit| &explicit.0)
            .or_else(|| {
                previous
                    .filter(|previous| value == previous.script.encoding())
                    .map(|previous| &previous.script)
            })
            .or_else(|| crate::i18n::native_text(value));
        let Some(script) = script else {
            if let Some(previous) = previous {
                if font.font == previous.applied.font {
                    let mut original = font.clone();
                    original.font = previous.original.font.clone();
                    original.font_features = previous.original.font_features.clone();
                    font.set_if_neq(original);
                }
                let mut entity = commands.entity(entity);
                entity.remove::<Rendered>();
                if previous.accessible {
                    entity.remove::<AccessibleLabel>();
                }
            }
            continue;
        };
        if previous.is_some_and(|previous| {
            Arc::ptr_eq(script, &previous.script)
                && value == script.encoding()
                && *font == previous.applied
        }) {
            continue;
        }
        let handle = native_font
            .0
            .get_or_insert_with(|| {
                assets.add(Font {
                    data: fontique::Blob::new(Arc::new(lince_ithkuil::FONT_BYTES)),
                    alias: String::new(),
                })
            })
            .clone();
        let mut original = font.clone();
        if let Some(previous) = previous {
            if original.font == previous.applied.font {
                original.font = previous.original.font.clone();
                original.font_features = previous.original.font_features.clone();
            }
        }
        let mut applied = original.clone();
        applied.font = handle.into();
        applied.font_features = FontFeatures::from([
            FontFeatureTag::STANDARD_LIGATURES,
            FontFeatureTag::CONTEXTUAL_LIGATURES,
            FontFeatureTag::CONTEXTUAL_ALTERNATES,
        ]);
        if value != script.encoding() {
            let value = text.get_text_mut();
            value.clear();
            value.push_str(script.encoding());
        }
        font.set_if_neq(applied.clone());
        let owned_label =
            previous.is_some_and(|previous| previous.accessible) || accessible.is_none();
        let mut entity = commands.entity(entity);
        if owned_label {
            entity.insert(AccessibleLabel::new(script.romanization()));
        }
        entity.insert(Rendered {
            script: script.clone(),
            original,
            applied,
            accessible: owned_label,
        });
    }
}

pub struct IthkuilPlugin;

impl Plugin for IthkuilPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptFont>().add_systems(
            PostUpdate,
            (labels::<Text>, labels::<TextSpan>)
                .chain()
                .before(bevy::text::load_font_assets_into_font_collection)
                .before(bevy::text::detect_text_needs_rerender)
                .before(bevy::ui::UiSystems::Content),
        );
    }
}

#[cfg(test)]
mod tests;
