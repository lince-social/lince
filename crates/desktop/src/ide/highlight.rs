use super::*;
use bevy::{
    render::{Extract, ExtractSchedule, RenderApp},
    text::{EditableTextGeneration, TextLayoutInfo},
    ui_render::{ExtractedUiItem, ExtractedUiNodes, RenderUiSystems},
};
use lince_editor::language::Style;

#[derive(Component, Default)]
pub(super) struct Syntax {
    pub language: Option<&'static str>,
    text: String,
    spans: Vec<(std::ops::Range<usize>, Style)>,
    colors: Vec<Option<LinearRgba>>,
}

#[derive(Resource, Default)]
struct Start(usize);

pub(super) fn install(app: &mut App) {
    app.add_systems(Last, update);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render.init_resource::<Start>().add_systems(
            ExtractSchedule,
            (
                begin.before(bevy::ui_render::extract_text_sections),
                extract.after(bevy::ui_render::extract_text_sections),
            )
                .in_set(RenderUiSystems::ExtractText),
        );
    }
}

fn update(
    mut editors: Query<(
        Ref<EditableText>,
        Ref<EditableTextGeneration>,
        &TextLayoutInfo,
        Ref<TextColor>,
        Option<&bevy::text::TextCursorStyle>,
        &mut Syntax,
    )>,
) {
    for (input, generation, info, foreground, cursor, mut syntax) in &mut editors {
        if !input.is_changed()
            && !generation.is_changed()
            && !syntax.is_changed()
            && !foreground.is_changed()
        {
            continue;
        }
        if input.is_composing() || syntax.language.is_none() {
            syntax.colors.clear();
            syntax.text.clear();
            continue;
        }
        let text = input.editor.raw_text();
        if text != syntax.text || syntax.is_changed() {
            syntax.spans = lince_editor::language::highlight(text, syntax.language.unwrap());
            syntax.text = text.into();
        }
        let Some(layout) = input.editor.try_layout() else {
            syntax.colors.clear();
            continue;
        };
        let mut colors = Vec::with_capacity(info.glyphs.len());
        let foreground = foreground.0.to_linear();
        let light_background = foreground.red + foreground.green + foreground.blue < 1.0;
        for line in layout.lines() {
            for run in line.runs() {
                for cluster in run.visual_clusters() {
                    let offset = cluster.text_range().start;
                    let index = syntax
                        .spans
                        .partition_point(|(range, _)| range.end <= offset);
                    let style = syntax
                        .spans
                        .get(index)
                        .filter(|(range, _)| range.contains(&offset))
                        .map(|(_, style)| *style);
                    for _ in cluster.glyphs() {
                        colors.push(style.map(|style| color(style, light_background)));
                    }
                }
            }
        }
        if colors.len() != info.glyphs.len() {
            syntax.colors.clear();
            continue;
        }
        for (color, glyph) in colors.iter_mut().zip(&info.glyphs) {
            if !glyph.atlas_info.is_alpha_mask
                || cursor.is_some_and(|cursor| cursor.selected_text_color.is_some())
                    && info
                        .selection_rects
                        .iter()
                        .any(|rect| rect.contains(glyph.position))
            {
                *color = None;
            }
        }
        syntax.colors = colors;
    }
}

fn color(style: Style, light: bool) -> LinearRgba {
    let [r, g, b] = match (style, light) {
        (Style::Keyword, false) => [0.55, 0.72, 1.0],
        (Style::String, false) => [0.62, 0.82, 0.55],
        (Style::Number, false) => [0.93, 0.70, 0.49],
        (Style::Comment, false) => [0.57, 0.64, 0.57],
        (Style::Keyword, true) => [0.18, 0.25, 0.67],
        (Style::String, true) => [0.20, 0.42, 0.20],
        (Style::Number, true) => [0.57, 0.25, 0.10],
        (Style::Comment, true) => [0.40, 0.45, 0.40],
    };
    Color::srgb(r, g, b).to_linear()
}

fn begin(nodes: Res<ExtractedUiNodes>, mut start: ResMut<Start>) {
    start.0 = nodes.uinodes.len();
}

fn extract(
    mut nodes: ResMut<ExtractedUiNodes>,
    start: Res<Start>,
    colors: Extract<Query<&Syntax>>,
) {
    if colors.iter().all(|syntax| syntax.colors.is_empty()) {
        return;
    }
    let mut offsets = BTreeMap::new();
    let nodes = &mut *nodes;
    for node in nodes.uinodes.iter().skip(start.0) {
        let ExtractedUiItem::Glyphs { range } = &node.item else {
            continue;
        };
        let entity = node.main_entity.id();
        let Ok(syntax) = colors.get(entity) else {
            continue;
        };
        let offset = offsets.entry(entity).or_insert(0);
        for (glyph, color) in nodes.glyphs[range.clone()]
            .iter_mut()
            .zip(syntax.colors.iter().skip(*offset))
        {
            if let Some(color) = color {
                glyph.color = *color;
            }
        }
        *offset += range.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        ecs::system::RunSystemOnce,
        text::{FontCx, GlyphAtlasInfo, LayoutCx, PositionedGlyph},
    };

    #[test]
    fn highlighting_colors_the_existing_shaped_glyphs_without_changing_text() {
        let mut fonts = FontCx::default();
        let font = Font::from_bytes(
            include_bytes!("../../../../institute/assets/fonts/DejaVuSans/DejaVuSansMono.ttf")
                .to_vec(),
        );
        fonts.collection.register_fonts(font.data, None);
        fonts.set_sans_serif_family("DejaVu Sans Mono").unwrap();
        let mut layouts = LayoutCx::default();
        let mut input = EditableText::new("let value = \"text\"; // note");
        input.pending_edits.clear();
        let layout = input.editor.layout(&mut fonts.context, &mut layouts.0);
        let count = layout
            .lines()
            .map(|line| {
                line.runs()
                    .map(|run| {
                        run.visual_clusters()
                            .map(|cluster| cluster.glyphs().count())
                            .sum::<usize>()
                    })
                    .sum::<usize>()
            })
            .sum::<usize>();
        assert!(count > 0);
        let glyphs = (0..count)
            .map(|_| PositionedGlyph {
                position: Vec2::ZERO,
                atlas_info: GlyphAtlasInfo {
                    texture: Default::default(),
                    rect: Rect::default(),
                    offset: Vec2::ZERO,
                    is_alpha_mask: true,
                },
                section_index: 0,
                line_index: 0,
            })
            .collect();
        let mut world = World::new();
        let entity = world
            .spawn((
                input,
                TextLayoutInfo {
                    glyphs,
                    ..default()
                },
                Syntax {
                    language: Some("rust"),
                    ..default()
                },
            ))
            .id();
        world.run_system_once(update).unwrap();
        let syntax = world.get::<Syntax>(entity).unwrap();
        assert_eq!(syntax.colors.len(), count);
        assert!(syntax.colors.iter().any(Option::is_some));
        assert!(syntax.colors.iter().any(Option::is_none));
        assert_eq!(
            world
                .get::<EditableText>(entity)
                .unwrap()
                .value()
                .to_string(),
            "let value = \"text\"; // note"
        );
    }
}
