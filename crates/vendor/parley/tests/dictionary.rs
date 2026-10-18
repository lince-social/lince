#![doc = include_str!("../CREDITS.txt")]

use parley::{FontContext, FontFamily, Layout, LayoutContext, StyleProperty, WordBreak};
use peniko::Blob;
use std::{path::Path, sync::Arc};

#[test]
fn japanese_dictionary_words_have_boundaries_when_character_line_breaks_are_disabled() {
    let mut fonts = FontContext::new();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/assets/fonts/NotoSansMonoCJK/NotoSansMonoCJKjp-Regular.otf");
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(std::fs::read(path).unwrap())), None);
    let text = "うなぎうなじ";
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(
        "Noto Sans Mono CJK JP",
    )));
    builder.push_default(StyleProperty::WordBreak(WordBreak::KeepAll));
    let mut layout: Layout<()> = builder.build(text);
    layout.break_all_lines(None);
    let mut boundaries = Vec::new();
    for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                if cluster.is_word_boundary() {
                    boundaries.push(cluster.text_range().start);
                }
            }
        }
    }
    assert_eq!(boundaries, [0, "うなぎ".len()]);
}
