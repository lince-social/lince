// non-lean-yet

//! Native Ithkuil text for Bevy labels and spans.

use crate::theme::bundled_font;
use bevy::{
    prelude::*,
    text::{
        EditableText, FontFeatureTag, FontFeatures, FontSource, TextSection,
        detect_text_needs_rerender, load_font_assets_into_font_collection,
    },
    ui::{UiSystems, prelude::AccessibleLabel},
};
use lince_ithkuil::FONT_BYTES;
use std::sync::Arc;

pub use lince_ithkuil::{Error, Script, render};

/// A pre-rendered native script shared by labels or spans.
#[derive(Component, Clone)]
pub struct ScriptText(pub Arc<Script>);

impl ScriptText {
    /// Renders the romanized label in native script.
    ///
    /// # Errors
    ///
    /// Returns an error if the renderer rejects the romanization.
    pub fn parse(romanization: &str) -> Result<Self, Error> {
        render(romanization).map(Arc::new).map(Self)
    }
}

#[derive(Component)]
struct Rendered {
    script: Arc<Script>,
    original: ScriptStyle,
    applied: TextFont,
    accessible: bool,
}

#[derive(Clone)]
struct ScriptStyle {
    font: FontSource,
    features: FontFeatures,
}

#[derive(Resource, Default)]
struct ScriptFont(Option<Handle<Font>>);

type LabelQuery<'w, 's, T> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut T,
        &'static mut TextFont,
        Option<&'static ScriptText>,
        Option<&'static Rendered>,
        Option<&'static AccessibleLabel>,
    ),
    (
        Without<EditableText>,
        Or<(Changed<T>, Changed<TextFont>, Changed<ScriptText>)>,
    ),
>;

fn labels<T: TextSection>(
    mut commands: Commands,
    mut assets: ResMut<Assets<Font>>,
    mut native_font: ResMut<ScriptFont>,
    mut texts: LabelQuery<T>,
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
        if let Some(script) = script
            && previous.is_some_and(|previous| {
                Arc::ptr_eq(script, &previous.script)
                    && value == script.encoding()
                    && *font == previous.applied
            })
        {
            continue;
        }
        let previous_style = previous
            .filter(|previous| font.font == previous.applied.font)
            .map(|previous| &previous.original);
        let Some(script) = script else {
            if let Some(previous) = previous {
                if let Some(style) = previous_style {
                    let mut original = font.clone();
                    original.font.clone_from(&style.font);
                    original.font_features.clone_from(&style.features);
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
        let handle = native_font
            .0
            .get_or_insert_with(|| assets.add(bundled_font(FONT_BYTES)))
            .clone();
        let original = previous_style.cloned().unwrap_or_else(|| ScriptStyle {
            font: font.font.clone(),
            features: font.font_features.clone(),
        });
        let mut applied = font.clone();
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

/// Shapes native labels before Bevy lays out text and leaves editors untouched.
pub struct IthkuilPlugin;

impl Plugin for IthkuilPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptFont>().add_systems(
            PostUpdate,
            (labels::<Text>, labels::<TextSpan>)
                .chain()
                .before(load_font_assets_into_font_collection)
                .before(detect_text_needs_rerender)
                .before(UiSystems::Content),
        );
    }
}

#[cfg(test)]
mod tests;
