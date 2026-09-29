use super::*;
pub(super) use lince_interface::markup::parse;

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
