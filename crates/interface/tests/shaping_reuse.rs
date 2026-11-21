// non-lean-yet
// Scope: fn register, fn fonts, struct Options, fn build.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

#![cfg(test)]

#![cfg(feature = "ui")]

use fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, SourceCache};
use parley::{
    Affinity, Alignment, AlignmentOptions, FontContext, FontFamily, FontFeatures, FontVariations,
    InlineBox, InlineBoxKind, Layout, LayoutContext, LineHeight, StyleProperty, WordBreak,
    editing::Cursor,
};
use std::{borrow::Cow, path::Path, sync::Arc};

fn register(fonts: &mut FontContext, file: &str, name: Option<&str>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/assets/fonts")
        .join(file);
    let data = Blob::new(Arc::new(std::fs::read(path).unwrap()));
    assert!(
        !fonts
            .collection
            .register_fonts(
                data,
                name.map(|name| FontInfoOverride {
                    family_name: Some(name),
                    ..Default::default()
                })
            )
            .is_empty()
    );
}

fn fonts() -> FontContext {
    let mut fonts = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    for file in [
        "Lato/Lato-Regular.ttf",
        "Lato/Lato-Bold.ttf",
        "DejaVuSans/DejaVuSans.ttf",
        "NotoSansMonoCJK/NotoSansMonoCJKjp-Regular.otf",
        "Aleo/Aleo-VariableFont_wght.ttf",
    ] {
        register(&mut fonts, file, None);
    }
    fonts
}

#[derive(Clone)]
struct Options<'a> {
    family: &'a str,
    size: f32,
    scale: f32,
    quantize: bool,
    features: &'a str,
    variations: &'a str,
    locale: &'a str,
    brush: u32,
    spacing: f32,
    width: Option<f32>,
    inline: bool,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            family: "Lato, 'DejaVu Sans', 'Noto Sans Mono CJK JP'",
            size: 16.,
            scale: 1.,
            quantize: true,
            features: "",
            variations: "",
            locale: "en",
            brush: 1,
            spacing: 0.,
            width: Some(210.),
            inline: false,
        }
    }
}

fn build(
    fonts: &mut FontContext,
    context: &mut LayoutContext<u32>,
    text: &str,
    options: &Options<'_>,
) -> Layout<u32> {
    let mut builder = context.ranged_builder(fonts, text, options.scale, options.quantize);
    builder.push_default(FontFamily::Source(Cow::Borrowed(options.family)));
    builder.push_default(StyleProperty::FontSize(options.size));
    builder.push_default(FontFeatures::from(options.features));
    builder.push_default(FontVariations::from(options.variations));
    builder.push_default(StyleProperty::Locale(Some(options.locale.parse().unwrap())));
    builder.push_default(StyleProperty::Brush(options.brush));
    builder.push_default(StyleProperty::LetterSpacing(options.spacing));
    builder.push_default(LineHeight::FontSizeRelative(1.3));
    builder.push_default(StyleProperty::WordBreak(WordBreak::Normal));
    if let Some((split, _)) = text.char_indices().nth(3) {
        builder.push(StyleProperty::Brush(options.brush + 1), split..);
        builder.push(StyleProperty::Underline(true), split..);
    }
    if options.inline {
        builder.push_inline_box(InlineBox {
            id: options.brush.into(),
            kind: InlineBoxKind::InFlow,
            index: 0,
            width: 23.,
            height: 18.,
        });
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(options.width);
    layout.align(Alignment::Start, AlignmentOptions::default());
    layout
}

fn same(a: &Layout<u32>, b: &Layout<u32>, text: &str) {
    assert_eq!(
        (
            a.scale(),
            a.width(),
            a.full_width(),
            a.height(),
            a.len(),
            a.is_rtl()
        ),
        (
            b.scale(),
            b.width(),
            b.full_width(),
            b.height(),
            b.len(),
            b.is_rtl()
        )
    );
    assert_eq!(a.styles(), b.styles());
    assert_eq!(a.inline_boxes(), b.inline_boxes());
    for (a, b) in a.lines().zip(b.lines()) {
        assert_eq!(
            (a.metrics(), a.text_range(), a.break_reason(), a.len()),
            (b.metrics(), b.text_range(), b.break_reason(), b.len())
        );
        let ar: Vec<_> = a.runs().collect();
        let br: Vec<_> = b.runs().collect();
        assert_eq!(ar.len(), br.len());
        for (a, b) in ar.into_iter().zip(br) {
            assert_eq!(a.font(), b.font());
            assert_eq!(a.font_attrs(), b.font_attrs());
            assert_eq!(a.synthesis(), b.synthesis());
            assert_eq!(a.normalized_coords(), b.normalized_coords());
            assert_eq!(a.metrics(), b.metrics());
            assert_eq!(
                (
                    a.font_size(),
                    a.advance(),
                    a.text_range(),
                    a.is_rtl(),
                    a.len()
                ),
                (
                    b.font_size(),
                    b.advance(),
                    b.text_range(),
                    b.is_rtl(),
                    b.len()
                )
            );
            for (a, b) in a.clusters().zip(b.clusters()) {
                assert_eq!(
                    (
                        a.text_range(),
                        a.advance(),
                        a.is_rtl(),
                        a.is_word_boundary(),
                        a.is_line_break()
                    ),
                    (
                        b.text_range(),
                        b.advance(),
                        b.is_rtl(),
                        b.is_word_boundary(),
                        b.is_line_break()
                    )
                );
                assert_eq!(a.first_style(), b.first_style());
                assert_eq!(
                    a.glyphs().collect::<Vec<_>>(),
                    b.glyphs().collect::<Vec<_>>()
                );
            }
        }
    }
    for index in text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
    {
        for affinity in [Affinity::Upstream, Affinity::Downstream] {
            let ac = Cursor::from_byte_index(a, index, affinity);
            let bc = Cursor::from_byte_index(b, index, affinity);
            assert_eq!(ac, bc);
            assert_eq!(ac.geometry(a, 1.), bc.geometry(b, 1.));
        }
    }
}

fn compare(
    fonts: &mut FontContext,
    warm: &mut LayoutContext<u32>,
    text: &str,
    options: &Options<'_>,
) {
    let expected = build(fonts, &mut LayoutContext::new(), text, options);
    let actual = build(fonts, warm, text, options);
    same(&actual, &expected, text);
    let again = build(fonts, warm, text, options);
    same(&again, &expected, text);
}

#[test]
fn reused_glyphs_preserve_unicode_clusters_and_cursors() {
    let mut fonts = fonts();
    let mut warm = LayoutContext::new();
    for text in [
        "office affine café",
        "e\u{301} e\u{301} café 日本語",
        "مرحبا بالعالم",
        "abc אבג 123 مرحبا",
        "line one\nline two\tend",
        "",
        "  ",
    ] {
        compare(&mut fonts, &mut warm, text, &Options::default());
    }
}

#[test]
fn shaping_properties_do_not_alias() {
    let mut fonts = fonts();
    let mut warm = LayoutContext::new();
    for family in ["Lato", "Aleo"] {
        for scale in [1., 1.25, 2.] {
            for size in [11., 24.] {
                for features in ["'liga' 0", "'liga' 1"] {
                    for variations in ["'wght' 100", "'wght' 900"] {
                        for locale in ["en", "tr"] {
                            compare(
                                &mut fonts,
                                &mut warm,
                                "office affine café",
                                &Options {
                                    family,
                                    scale,
                                    size,
                                    features,
                                    variations,
                                    locale,
                                    ..Options::default()
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn brushes_spacing_inline_boxes_and_wrapping_stay_independent() {
    let mut fonts = fonts();
    let mut warm = LayoutContext::new();
    let text = "office affine café 日本語";
    let mut first = build(&mut fonts, &mut warm, text, &Options::default());
    for brush in [9, 45] {
        for spacing in [0., 2.] {
            for width in [Some(65.), Some(410.), None] {
                for quantize in [false, true] {
                    compare(
                        &mut fonts,
                        &mut warm,
                        text,
                        &Options {
                            brush,
                            spacing,
                            width,
                            quantize,
                            inline: true,
                            ..Options::default()
                        },
                    );
                }
            }
        }
    }
    first.break_all_lines(Some(80.));
    first.align(Alignment::End, AlignmentOptions::default());
    compare(&mut fonts, &mut warm, text, &Options::default());
}

#[test]
fn replacement_fonts_and_other_font_contexts_do_not_reuse_old_glyphs() {
    let mut fonts = fonts();
    let mut warm = LayoutContext::new();
    let options = Options {
        family: "Replacement",
        width: None,
        ..Options::default()
    };
    let text = "office affine café";
    register(&mut fonts, "Lato/Lato-Regular.ttf", Some("Replacement"));
    let before = build(&mut fonts, &mut warm, text, &options);
    let family = fonts.collection.family_id("Replacement").unwrap();
    assert!(fonts.collection.unregister_font(
        family,
        fontique::FontWidth::NORMAL,
        fontique::FontStyle::Normal,
        fontique::FontWeight::NORMAL
    ));
    register(
        &mut fonts,
        "DejaVuSans/DejaVuSansMono.ttf",
        Some("Replacement"),
    );
    compare(&mut fonts, &mut warm, text, &options);
    let after = build(&mut fonts, &mut warm, text, &options);
    assert_ne!(before.full_width(), after.full_width());
    let mut other = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    register(&mut other, "Lato/Lato-Bold.ttf", Some("Replacement"));
    compare(&mut other, &mut warm, text, &options);
}

#[test]
fn distinct_text_eviction_and_long_uncached_text_preserve_results() {
    let mut fonts = fonts();
    let mut warm = LayoutContext::new();
    let options = Options::default();
    for index in 0..100 {
        compare(
            &mut fonts,
            &mut warm,
            &format!("Different words {index} café"),
            &options,
        );
    }
    compare(
        &mut fonts,
        &mut warm,
        &"office affine café ".repeat(90),
        &options,
    );
    compare(&mut fonts, &mut warm, "Different words 0 café", &options);
}
