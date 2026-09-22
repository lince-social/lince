use super::*;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}
#[derive(Debug, Default, PartialEq)]
pub(super) struct Block {
    pub runs: Vec<Run>,
    pub heading: u8,
    pub code: Option<String>,
    pub quote: bool,
    pub image: Option<String>,
}

pub(super) fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Run::default();
    let mut numbered = Vec::new();
    let mut quotes = 0usize;
    let flush = |blocks: &mut Vec<Block>, block: &mut Block| {
        if !block.runs.is_empty() || block.image.is_some() {
            blocks.push(std::mem::take(block));
        }
    };
    let push = |block: &mut Block, style: &Run, value: &str| {
        if let Some(last) = block.runs.last_mut()
            && last.bold == style.bold
            && last.italic == style.italic
            && last.code == style.code
            && last.strike == style.strike
            && last.link == style.link
        {
            last.text.push_str(value);
        } else {
            block.runs.push(Run {
                text: value.into(),
                ..style.clone()
            });
        }
    };
    for event in Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
    ) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                flush(&mut blocks, &mut block);
                block.image = Some(dest_url.into_string());
            }
            Event::End(TagEnd::Image) => flush(&mut blocks, &mut block),
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut blocks, &mut block);
                block.heading = level as u8;
            }
            Event::Start(Tag::Paragraph) => {
                flush(&mut blocks, &mut block);
                block.quote = quotes > 0;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut blocks, &mut block);
                block.code = Some(match kind {
                    CodeBlockKind::Fenced(language) => language.into_string(),
                    CodeBlockKind::Indented => String::new(),
                });
                style.code = true;
            }
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.into_string()),
            Event::End(TagEnd::Link) => style.link = None,
            Event::Start(Tag::List(first)) => numbered.push(first),
            Event::End(TagEnd::List(_)) => {
                numbered.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut blocks, &mut block);
                let marker = match numbered.last_mut() {
                    Some(Some(value)) => {
                        let marker = format!("{value}. ");
                        *value += 1;
                        marker
                    }
                    _ => "• ".into(),
                };
                push(&mut block, &style, &marker);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quotes += 1;
                block.quote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quotes = quotes.saturating_sub(1);
                block.quote = quotes > 0;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush(&mut blocks, &mut block);
                style.code = false;
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableRow,
            ) => flush(&mut blocks, &mut block),
            Event::End(TagEnd::TableCell) => push(&mut block, &style, "   │   "),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                push(&mut block, &style, &text)
            }
            Event::Code(text) => push(
                &mut block,
                &Run {
                    code: true,
                    ..style.clone()
                },
                &text,
            ),
            Event::SoftBreak => push(&mut block, &style, " "),
            Event::HardBreak => push(&mut block, &style, "\n"),
            Event::TaskListMarker(done) => {
                push(&mut block, &style, if done { "[x] " } else { "[ ] " })
            }
            Event::Rule => {
                flush(&mut blocks, &mut block);
                push(&mut block, &style, "────────────");
                flush(&mut blocks, &mut block);
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut block);
    for block in &mut blocks {
        if block.code.is_none() {
            block.runs = std::mem::take(&mut block.runs)
                .into_iter()
                .flat_map(links)
                .flat_map(slugs)
                .collect();
        }
    }
    blocks
}

fn links(run: Run) -> Vec<Run> {
    if run.code || run.link.is_some() {
        return vec![run];
    }
    let mut out = Vec::new();
    let mut rest = run.text.as_str();
    while let Some(start) = rest.find("[[") {
        let Some(end) = rest[start + 2..].find("]]").map(|end| end + start + 2) else {
            break;
        };
        let (title, reference) = rest[start + 2..end]
            .split_once('|')
            .unwrap_or((&rest[start + 2..end], &rest[start + 2..end]));
        if start > 0 {
            out.push(Run {
                text: rest[..start].into(),
                ..run.clone()
            });
        }
        out.push(Run {
            text: title.into(),
            link: Some(reference.into()),
            ..run.clone()
        });
        rest = &rest[end + 2..];
    }
    if !rest.is_empty() {
        out.push(Run {
            text: rest.into(),
            ..run.clone()
        });
    }
    out
}

fn slugs(run: Run) -> Vec<Run> {
    if run.code || run.link.is_some() {
        return vec![run];
    }
    let mut out = Vec::new();
    let mut pending = 0;
    for (start, character) in run.text.char_indices() {
        if character != '@'
            || run.text[..start]
                .chars()
                .next_back()
                .is_some_and(|previous| previous.is_alphanumeric() || previous == '_')
        {
            continue;
        }
        let end = run.text[start + 1..]
            .char_indices()
            .find(|(_, character)| {
                !character.is_alphanumeric() && *character != '-' && *character != '_'
            })
            .map_or(run.text.len(), |(end, _)| start + 1 + end);
        if end == start + 1 {
            continue;
        }
        if pending < start {
            out.push(Run {
                text: run.text[pending..start].into(),
                ..run.clone()
            });
        }
        out.push(Run {
            text: run.text[start..end].into(),
            link: Some(run.text[start + 1..end].into()),
            ..run.clone()
        });
        pending = end;
    }
    if pending < run.text.len() {
        out.push(Run {
            text: run.text[pending..].into(),
            ..run.clone()
        });
    }
    out
}

pub(super) fn render(
    world: &mut World,
    parent: Entity,
    source: &str,
    context: &Context,
    shaders: Vec<Entity>,
) {
    world.init_resource::<Fonts>();
    let mut shaders = shaders.into_iter();
    for block in parse(source) {
        if block
            .code
            .as_deref()
            .is_some_and(|language| language.trim().eq_ignore_ascii_case("wgsl"))
        {
            shader::spawn(
                world,
                parent,
                &block
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>(),
                shaders.next(),
            );
            continue;
        }
        if let Some(source) = &block.image {
            pictures::spawn(
                world,
                parent,
                source,
                &block
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>(),
            );
            continue;
        }
        if block
            .code
            .as_deref()
            .is_some_and(|lang| lang.trim().eq_ignore_ascii_case("mermaid"))
        {
            diagram::spawn(
                world,
                parent,
                &block
                    .runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>(),
            );
            continue;
        }
        if block.code.is_some() {
            let text = block
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>();
            let entity = crate::edit_mode::label(world, parent, &text, 14.0);
            world.entity_mut(entity).insert(Node {
                width: percent(100),
                padding: UiRect::all(px(8)),
                flex_shrink: 0.0,
                ..default()
            });
            continue;
        }
        let size = match block.heading {
            1 => 26.0,
            2 => 23.0,
            3.. => 20.0,
            _ => 16.0,
        };
        let row = crate::edit_mode::label(world, parent, "", size);
        world.get_mut::<Node>(row).unwrap().width = percent(100);
        if block.quote {
            let mut node = world.get_mut::<Node>(row).unwrap();
            node.padding.left = px(10);
            node.border.left = px(2);
            world
                .entity_mut(row)
                .insert(crate::token_style::border(Token::Accent));
        }
        let mut links = Vec::new();
        for (index, run) in block.runs.into_iter().enumerate() {
            let reference = run.link.clone();
            let mut font = world.resource::<crate::theme::Typography>().text(size);
            if run.bold || block.heading > 0 {
                font.font = world.resource::<Fonts>().bold.clone().into();
            } else if run.italic {
                font.font = world.resource::<Fonts>().italic.clone().into();
            }
            let entity = world
                .spawn((
                    TextSpan::new(&run.text),
                    font,
                    crate::token_style::text(if reference.is_some() {
                        Token::Accent
                    } else {
                        Token::Ink
                    }),
                    ChildOf(row),
                ))
                .id();
            if run.code {
                world
                    .entity_mut(entity)
                    .insert(bevy::text::TextBackgroundColor(Color::srgba(
                        0.5, 0.5, 0.5, 0.15,
                    )));
            }
            if run.strike {
                world.entity_mut(entity).insert(bevy::text::Strikethrough);
            }
            if let Some(reference) = reference {
                if !reference.contains("://") {
                    world.entity_mut(entity).insert(bevy::text::Underline);
                    links.push((
                        index + 1,
                        Link {
                            reference,
                            context: context.clone(),
                        },
                    ));
                }
            }
        }
        if !links.is_empty() {
            let mut accessibility = accesskit::Node::new(accesskit::Role::Link);
            accessibility.set_label(format!(
                "Record links: {}. Arrow keys select a link; Enter opens it.",
                links
                    .iter()
                    .map(|(_, link)| link.reference.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            world
                .entity_mut(row)
                .insert((
                    ParagraphLinks { links, selected: 0 },
                    bevy::input_focus::tab_navigation::TabIndex(0),
                    bevy::a11y::AccessibilityNode::from(accessibility),
                    crate::icons::Tooltip("Record links: ← / → to select, Enter to open".into()),
                ))
                .observe(click)
                .observe(keyboard);
        }
    }
    for shader in shaders {
        world.despawn(shader);
    }
}

#[derive(Component)]
struct ParagraphLinks {
    links: Vec<(usize, Link)>,
    selected: usize,
}

fn click(
    mut event: On<Pointer<Click>>,
    paragraphs: Query<(
        &ParagraphLinks,
        &bevy::text::TextLayoutInfo,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    mut commands: Commands,
) {
    if event.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok((links, layout, node, transform)) = paragraphs.get(event.entity) else {
        return;
    };
    let Some(inverse) = transform.try_inverse() else {
        return;
    };
    let point = (inverse
        .transform_point2(event.pointer_location.position / node.inverse_scale_factor())
        + node.size() / 2.0)
        / layout.scale_factor;
    let link = layout
        .run_geometry
        .iter()
        .find(|run| run.bounds.contains(point))
        .and_then(|run| {
            links
                .links
                .iter()
                .find(|(section, _)| *section == run.section_index)
        })
        .map(|(_, link)| link.clone());
    if let Some(link) = link {
        event.propagate(false);
        commands.queue(move |world: &mut World| link.apply(world, link.context.owner));
    }
}

fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    mut paragraphs: Query<&mut ParagraphLinks>,
    mut commands: Commands,
) {
    if !event.input.state.is_pressed() {
        return;
    }
    let Ok(mut links) = paragraphs.get_mut(event.focused_entity) else {
        return;
    };
    match event.input.key_code {
        KeyCode::ArrowRight => links.selected = (links.selected + 1) % links.links.len(),
        KeyCode::ArrowLeft => {
            links.selected = (links.selected + links.links.len() - 1) % links.links.len()
        }
        KeyCode::Enter => {
            let link = links.links[links.selected].1.clone();
            commands.queue(move |world: &mut World| link.apply(world, link.context.owner));
        }
        _ => return,
    }
    event.propagate(false);
}
