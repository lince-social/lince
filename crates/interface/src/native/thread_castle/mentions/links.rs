// non-lean-yet
// Scope: struct Geometry, enum Update, struct Prepared, fn paint.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 170, 182, 222,
//   226, 228, 268, 284.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (40, 100).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   205, 222.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 116.

use super::*;
use bevy::ecs::system::InMut;

#[derive(Clone, Copy)]
struct Geometry {
    origin: Vec2,
    size: Vec2,
}

impl Geometry {
    fn node(self) -> Node {
        Node {
            position_type: PositionType::Absolute,
            left: px(self.origin.x),
            top: px(self.origin.y),
            width: px(self.size.x),
            height: px(self.size.y),
            border: UiRect::bottom(px(1)),
            ..default()
        }
    }
}

enum Update {
    Geometry(Entity, Geometry),
    Create(InlineLink, Geometry, String),
}

#[derive(Default)]
struct Prepared {
    updates: Vec<Update>,
    stale: Vec<Entity>,
    boxes: Vec<(usize, Vec2, Vec2, usize)>,
}

pub(super) fn paint(world: &mut World) {
    let _paint = nucleus::performance::Span::new("mention-link-presentation");
    world
        .run_system_cached(paint_current)
        .expect("present current mention links");
}

fn paint_current(
    world: &mut World,
    selected: &mut QueryState<
        (Entity, &'static ChildOf, &'static ComputedNode),
        With<SelectedOption>,
    >,
    mut selections: Local<Vec<(Entity, Entity)>>,
    mut prepared: Local<Prepared>,
) {
    selections.clear();
    selections.extend(
        selected
            .iter(world)
            .map(|(entity, parent, _)| (entity, parent.parent())),
    );
    if selections.is_empty() {
        selections.shrink_to_fit();
    }
    for (entity, popup) in selections.drain(..) {
        let Some(children) = world.get::<Children>(popup) else {
            continue;
        };
        let mut top = 0.0;
        let mut selected = None;
        for child in children.iter() {
            let height = world
                .get::<ComputedNode>(child)
                .map_or(0.0, |node| node.size.y * node.inverse_scale_factor);
            if world.get::<SelectedOption>(child).is_some() {
                selected = Some((top, top + height));
            }
            top += height;
        }
        let height = world.get::<ComputedNode>(popup).map_or(0.0, |node| {
            node.content_box().height() * node.inverse_scale_factor
        });
        if height > 0.0
            && let Some((top, bottom)) = selected
            && let Some(mut scroll) = world.get_mut::<ScrollPosition>(popup)
        {
            let offset = if top < scroll.0.y {
                Some(top)
            } else if bottom > scroll.0.y + height {
                Some(bottom - height)
            } else {
                None
            };
            if let Some(offset) = offset
                && scroll.0.y != offset
            {
                scroll.0.y = offset;
            }
            world.entity_mut(entity).remove::<SelectedOption>();
        }
    }
    world
        .run_system_cached_with(prepare, &mut *prepared)
        .expect("prepare mention link geometry");
    for update in prepared.updates.drain(..) {
        match update {
            Update::Geometry(entity, geometry) => {
                world.entity_mut(entity).insert(geometry.node());
                nucleus::performance::add("mention-link-geometry-updates", 1);
            }
            Update::Create(link, geometry, label) => create(world, link, geometry, label),
        }
    }
    for entity in prepared.stale.drain(..) {
        world.despawn(entity);
    }
}

fn prepare(
    InMut(prepared): InMut<Prepared>,
    fonts: Option<ResMut<FontCx>>,
    layout: Option<ResMut<LayoutCx>>,
    links: Query<(Entity, &InlineLink, Option<&Node>)>,
    inputs: Query<(
        Entity,
        &Draft,
        Option<&EditableText>,
        Option<&ComputedNode>,
        Option<&bevy::ui::widget::TextScroll>,
    )>,
) {
    let _prepare = nucleus::performance::Span::new("mention-link-geometry-preparation");
    let Prepared {
        updates,
        stale,
        boxes,
    } = prepared;
    updates.clear();
    stale.clear();
    boxes.clear();
    let (Some(mut fonts), Some(mut layout)) = (fonts, layout) else {
        updates.shrink_to_fit();
        stale.shrink_to_fit();
        boxes.shrink_to_fit();
        return;
    };
    let mut old: HashMap<_, _> = links
        .iter()
        .map(|(entity, link, node)| {
            (
                (
                    link.input,
                    link.range.start,
                    link.range.end,
                    link.line,
                    link.uid.as_str(),
                ),
                (entity, node),
            )
        })
        .collect();
    let mut active = false;
    for (input, draft, text, computed, scroll) in &inputs {
        if draft.links.is_empty() {
            continue;
        }
        let Some(text) = text else {
            continue;
        };
        active = true;
        boxes.clear();
        let ready = draft.links.iter().enumerate().all(|(index, link)| {
            text.editor
                .try_range_geometry_with(link.range.clone(), |rect, line| {
                    boxes.push((
                        index,
                        Vec2::new(rect.x0 as f32, rect.y0 as f32),
                        Vec2::new(rect.x1 as f32, rect.y1 as f32),
                        line,
                    ));
                })
        });
        if !ready {
            nucleus::performance::add("mention-editor-clone-fallbacks", 1);
            boxes.clear();
            let mut editor = text.editor.clone();
            for (index, link) in draft.links.iter().enumerate() {
                editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .select_byte_range(link.range.start, link.range.end);
                editor.selection_geometry_with(|rect, line| {
                    boxes.push((
                        index,
                        Vec2::new(rect.x0 as f32, rect.y0 as f32),
                        Vec2::new(rect.x1 as f32, rect.y1 as f32),
                        line,
                    ));
                });
            }
        }
        let computed = computed.copied().unwrap_or_default();
        let scroll = scroll.map_or(Vec2::ZERO, |scroll| scroll.0);
        for (index, min, max, line) in boxes.drain(..) {
            let min = (min - scroll).max(Vec2::ZERO);
            let max = (max - scroll).min(computed.content_box().size());
            if max.x <= min.x || max.y <= min.y {
                continue;
            }
            let link = &draft.links[index];
            let geometry = Geometry {
                origin: (computed.padding.min_inset + min) * computed.inverse_scale_factor,
                size: (max - min) * computed.inverse_scale_factor,
            };
            let key = (
                input,
                link.range.start,
                link.range.end,
                line,
                link.uid.as_str(),
            );
            if let Some((entity, node)) = old.remove(&key) {
                if node != Some(&geometry.node()) {
                    updates.push(Update::Geometry(entity, geometry));
                }
            } else {
                let label = format!("Open {}", &text.value().to_cow()[link.range.clone()]);
                updates.push(Update::Create(
                    InlineLink {
                        input,
                        range: link.range.clone(),
                        line,
                        uid: link.uid.clone(),
                    },
                    geometry,
                    label,
                ));
            }
        }
    }
    stale.extend(old.into_values().map(|(entity, _)| entity));
    if updates.is_empty() {
        updates.shrink_to_fit();
    }
    if stale.is_empty() {
        stale.shrink_to_fit();
    }
    if !active {
        boxes.shrink_to_fit();
    }
}

fn create(world: &mut World, link: InlineLink, geometry: Geometry, label: String) {
    let mut accessibility = accesskit::Node::new(accesskit::Role::Link);
    accessibility.set_label(label);
    let input = link.input;
    world
        .spawn((
            geometry.node(),
            link,
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
            crate::native::icons::Tooltip("Open Record".into()),
            AccessibilityNode::from(accessibility),
            bevy::input_focus::tab_navigation::TabIndex(0),
            ChildOf(input),
        ))
        .observe(
            |mut event: On<PointerClick>, links: Query<&InlineLink>, mut commands: Commands| {
                if event.button != bevy::picking::pointer::PointerButton::Primary {
                    return;
                }
                if let Ok(link) = links.get(event.entity) {
                    let (input, uid) = (link.input, link.uid.clone());
                    event.propagate(false);
                    commands.queue(move |world: &mut World| open(world, input, &uid));
                }
            },
        )
        .observe(
            |mut event: On<
                bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>,
            >,
             links: Query<&InlineLink>,
             mut commands: Commands| {
                if event.input.state.is_pressed()
                    && matches!(event.input.key_code, KeyCode::Enter | KeyCode::NumpadEnter)
                    && let Ok(link) = links.get(event.focused_entity)
                {
                    let (input, uid) = (link.input, link.uid.clone());
                    event.propagate(false);
                    commands.queue(move |world: &mut World| open(world, input, &uid));
                }
            },
        );
    nucleus::performance::add("mention-inline-links-created", 1);
}
