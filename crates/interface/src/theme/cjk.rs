// non-lean-yet

use super::{bundled_font, register_fallbacks};
use crate::wake::WakeSignal;
use bevy::{
    prelude::*,
    text::{EditableText, FontCx},
};

static DATA: &[u8] = include_bytes!(
    "../../../../institute/assets/fonts/NotoSansMonoCJK/NotoSansMonoCJKjp-Regular.otf"
);
const FAMILY: &str = "Noto Sans Mono CJK JP";
const SCRIPTS: [[u8; 4]; 7] = [
    *b"Hani", *b"Hira", *b"Kana", *b"Hang", *b"Bopo", *b"Zyyy", *b"Zinh",
];

#[derive(Resource, Default)]
pub(super) struct Fallback(Option<Handle<Font>>);

fn needed(value: &str) -> bool {
    !value.is_ascii()
        && value.chars().any(|ch| {
            matches!(
                u32::from(ch),
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
    wake: Option<Res<WakeSignal>>,
) {
    if fallback.0.is_some()
        || !(texts.iter().any(|text| needed(&text.0))
            || spans.iter().any(|text| needed(&text.0))
            || inputs.iter().any(|text| needed(text.editor.raw_text())))
    {
        return;
    }
    fallback.0 = Some(fonts.add(bundled_font(DATA)));
    if let Some(wake) = wake {
        wake.ring();
    }
}

pub(super) fn configure(
    fallback: Res<Fallback>,
    fonts: Option<ResMut<FontCx>>,
    mut text_fonts: Query<&mut TextFont>,
) {
    if fallback.0.is_none() {
        return;
    }
    if let Some(mut fonts) = fonts
        && register_fallbacks(&mut fonts.collection, FAMILY, SCRIPTS)
    {
        for mut text_font in &mut text_fonts {
            text_font.set_changed();
        }
    }
}

#[cfg(test)]
mod tests;
