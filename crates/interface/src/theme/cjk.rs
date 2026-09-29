use bevy::{prelude::*, text::EditableText};
use std::sync::Arc;

static DATA: &[u8] = include_bytes!(
    "../../../../institute/assets/fonts/NotoSansMonoCJK/NotoSansMonoCJKjp-Regular.otf"
);
const FAMILY: &str = "Noto Sans Mono CJK JP";

#[derive(Resource, Default)]
pub(super) struct Fallback(Option<Handle<Font>>);

fn needed(value: &str) -> bool {
    !value.is_ascii()
        && value.chars().any(|ch| {
            matches!(
                ch as u32,
                0x1100..=0x11ff
                    | 0x2e80..=0x9fff
                    | 0xa960..=0xa97f
                    | 0xac00..=0xd7ff
                    | 0xf900..=0xfaff
                    | 0xfe10..=0xfe1f
                    | 0xfe30..=0xfe4f
                    | 0xff00..=0xffef
                    | 0x1b000..=0x1b16f
                    | 0x1f200..=0x1f2ff
                    | 0x20000..=0x323af
            )
        })
}

pub(super) fn load(
    mut fallback: ResMut<Fallback>,
    mut fonts: ResMut<Assets<Font>>,
    texts: Query<&Text, Changed<Text>>,
    spans: Query<&TextSpan, Changed<TextSpan>>,
    inputs: Query<&EditableText, Changed<EditableText>>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    if fallback.0.is_some()
        || !(texts.iter().any(|text| needed(&text.0))
            || spans.iter().any(|text| needed(&text.0))
            || inputs.iter().any(|text| needed(text.editor.raw_text())))
    {
        return;
    }
    fallback.0 = Some(fonts.add(Font {
        data: fontique::Blob::new(Arc::new(DATA)),
        alias: String::new(),
    }));
    if let Some(wake) = wake {
        wake.ring();
    }
}

pub(super) fn configure(
    fallback: Res<Fallback>,
    mut fonts: Option<ResMut<bevy::text::FontCx>>,
    mut text_fonts: Query<&mut TextFont>,
) {
    if fallback.0.is_none() {
        return;
    }
    let Some(fonts) = fonts.as_mut() else { return };
    let Some(id) = fonts.collection.family_id(FAMILY) else {
        return;
    };
    let mut changed = false;
    for tag in [
        *b"Hani", *b"Hira", *b"Kana", *b"Hang", *b"Bopo", *b"Zyyy", *b"Zinh",
    ] {
        let script = fontique::Script::from_bytes(tag);
        if !fonts
            .collection
            .fallback_families(script)
            .any(|family| family == id)
        {
            fonts.collection.append_fallbacks(script, [id].into_iter());
            changed = true;
        }
    }
    if changed {
        for mut text_font in &mut text_fonts {
            text_font.set_changed();
        }
    }
}

#[cfg(test)]
mod tests;
