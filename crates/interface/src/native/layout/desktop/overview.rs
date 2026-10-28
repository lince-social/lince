use super::*;
use crate::host::Direction;
use crate::native::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
    shortcuts::{Settings, Shortcut},
    topology::previews::{Preview, Previews},
};
use bevy::input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup};

pub(crate) mod settings;
mod dragging;
mod hover;
pub use hover::{acknowledge_hover, file_drag};
pub(super) use hover::active_file_drag;
pub(crate) use hover::{drop_allowed, file_event};

#[derive(Resource, Default)]
struct Overview(Option<Session>);

#[derive(Component)]
struct OverviewButton;

struct Session {
    root: Entity,
    output: Option<OutputId>,
    overlay: Entity,
    list: Entity,
    help: Entity,
    saved: focus::ReturnState,
    cards: Vec<Card>,
    selected: (u64, Option<[u8; 16]>),
    drag: Option<dragging::WindowDrag>,
    hover: Option<hover::FileHover>,
    hover_retired: Option<hover::Source>,
    hover_sequence: u64,
    focus_selection: bool,
}

struct Card {
    workspace: u64,
    entity: Entity,
    title: Entity,
    preview: Entity,
    image: Option<(UVec2, Handle<Image>)>,
    items: BTreeMap<[u8; 16], (Entity, Entity)>,
    order: Vec<[u8; 16]>,
}

#[derive(Clone, Copy)]
enum Change {
    Cancel,
    Select(u64, Option<[u8; 16]>),
    Commit,
    Navigate(Direction),
}

impl Action for Change {
    fn apply(&self, world: &mut World, _: Entity) {
        match *self {
            Self::Cancel => close(world, true),
            Self::Select(workspace, item) => {
                let root = world
                    .get_resource::<Overview>()
                    .and_then(|overview| overview.0.as_ref())
                    .map(|session| session.root);
                if let Some(root) = root {
                    DesktopAction::SelectPlacement { workspace, item }.apply(world, root);
                }
            }
            Self::Commit => {
                if let Some((root, (workspace, item))) = world
                    .get_resource::<Overview>()
                    .and_then(|overview| overview.0.as_ref())
                    .map(|session| (session.root, session.selected))
                {
                    DesktopAction::SelectPlacement { workspace, item }.apply(world, root);
                }
            }
            Self::Navigate(direction) => navigate(world, direction),
        }
    }
}

pub fn active(world: &World) -> bool {
    world
        .get_resource::<Overview>()
        .and_then(|overview| overview.0.as_ref())
        .is_some_and(|session| {
            world.get_entity(session.overlay).is_ok()
                && world
                    .get_resource::<crate::native::shortcuts::KeyboardModalOwner>()
                    .is_some_and(|owner| owner.0 == Some(session.overlay))
        })
}

fn output(world: &World, root: Entity) -> Option<OutputId> {
    world
        .get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
        .map(|views| views.focused)
}

pub(super) fn toggle(world: &mut World, root: Entity) {
    if active(world) {
        close(world, true);
        return;
    }
    if crate::native::shortcuts::keyboard_owned(world) {
        return;
    }
    let Some(saved) = focus::suspend(world, root) else {
        return;
    };
    crate::native::external_drop::suspend_hover(world, root);
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Previews>();
    world.init_resource::<Overview>();
    let workspace = world.get::<Workspaces>(root).unwrap().active;
    let focused = world
        .get::<Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|space| space.id == workspace)
        .and_then(|space| space.desktop.focused())
        .map(|id| id.0);
    let overlay = world
        .spawn((
            crate::native::inspection::InspectionExcluded,
            Name::new("desktop.overview"),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(12)),
                row_gap: px(10),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            GlobalZIndex(90),
            TabGroup::modal(),
            ChildOf(root),
        ))
        .id();
    let header = world
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(12),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(overlay),
        ))
        .id();
    let heading = output(world, root).map_or_else(
        || crate::i18n::Token::ShortcutOverview.text().to_owned(),
        |output| {
            format!(
                "{} · {} {}",
                crate::i18n::Token::ShortcutOverview.text(),
                crate::i18n::Token::OverviewMonitor.text(),
                output.0
            )
        },
    );
    let heading = label(world, header, &heading, 24.0);
    {
        let mut node = world.get_mut::<Node>(heading).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_width = px(0);
    }
    world.entity_mut(heading).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    button(
        world,
        header,
        crate::i18n::Token::Back.text(),
        Change::Cancel,
    );
    let help = label(world, overlay, "", 13.0);
    let list = world
        .spawn((
            Node {
                min_height: px(0),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                align_content: AlignContent::Start,
                align_items: AlignItems::Center,
                overflow: Overflow::scroll_y(),
                row_gap: px(12),
                column_gap: px(12),
                ..default()
            },
            ScrollPosition::default(),
            crate::native::edit_mode::FocusScroll,
            ChildOf(overlay),
        ))
        .observe(
            |mut event: On<Pointer<Scroll>>, mut positions: Query<&mut ScrollPosition>| {
                if let Ok(mut position) = positions.get_mut(event.entity) {
                    let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                    position.0.y -= event.y * multiplier;
                    event.propagate(false);
                }
            },
        )
        .id();
    let owner = output(world, root);
    world.resource_mut::<Overview>().0 = Some(Session {
        root,
        output: owner,
        overlay,
        list,
        help,
        saved,
        cards: Vec::new(),
        selected: (workspace, focused),
        drag: None,
        hover: None,
        hover_retired: None,
        hover_sequence: 0,
        focus_selection: false,
    });
    crate::native::shortcuts::own_keyboard(world, overlay);
    world
        .resource_mut::<InputFocus>()
        .set(overlay, FocusCause::Navigated);
    update(world);
    let target = world
        .get_resource::<Overview>()
        .and_then(|overview| overview.0.as_ref())
        .and_then(|session| {
            let card = session
                .cards
                .iter()
                .find(|card| card.workspace == session.selected.0)?;
            Some(
                session
                    .selected
                    .1
                    .and_then(|id| card.items.get(&id).map(|(button, _)| *button))
                    .unwrap_or(card.title),
            )
        });
    if let Some(target) = target {
        world
            .resource_mut::<InputFocus>()
            .set(target, FocusCause::Navigated);
    }
}

fn button(world: &mut World, parent: Entity, text: &str, change: Change) -> Entity {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::sand::FixedGeometry,
            OverviewButton,
            Outline {
                width: px(0),
                offset: px(2),
                color: crate::native::theme::PURPLE,
            },
            crate::native::token_style::OutlineToken(crate::native::tokens::Token::Accent),
            crate::native::token_style::border(crate::native::tokens::Token::SandBorder),
            crate::native::shortcuts::KeyboardModalTrigger,
            ActionButton::new(parent, crate::native::actions![change]),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    label(world, entity, text, 14.0);
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(text);
    entity
}

pub(super) fn item_name(world: &World, entity: Entity) -> String {
    let properties = world.get::<crate::native::area::RecordProperties>(entity);
    let property = |key: &str| {
        properties
            .and_then(|properties| properties.0.get(key))
            .and_then(serde_json::Value::as_str)
    };
    [
        world.get::<Name>(entity).map(Name::as_str),
        property("window_title"),
        property("head"),
        property("app_id"),
        world
            .get::<crate::native::sand_store::StoredSand>(entity)
            .map(|sand| sand.kind.name()),
    ]
    .into_iter()
    .flatten()
    .find_map(|value| {
        let value: String = value
            .chars()
            .filter(|character| !character.is_control())
            .take(160)
            .collect();
        (!value.trim().is_empty()).then(|| value.trim().to_owned())
    })
    .unwrap_or_else(|| "Item".into())
}

fn close(world: &mut World, restore: bool) {
    let Some(mut overview) = world.get_resource_mut::<Overview>() else {
        return;
    };
    let Some(session) = overview.0.take() else {
        return;
    };
    crate::native::shortcuts::release_keyboard(world, session.overlay);
    if let Some(mut previews) = world.get_resource_mut::<Previews>() {
        previews.0.retain(|preview| preview.root != session.root);
    }
    if let Ok(entity) = world.get_entity_mut(session.overlay) {
        entity.despawn();
    }
    if restore {
        focus::resume(world, session.root, session.saved);
    }
}

pub fn dismiss(world: &mut World) {
    close(world, true);
}

pub fn previewed_applications(world: &mut World) -> Vec<SurfaceId> {
    if !active(world) {
        return Vec::new();
    }
    world
        .query::<(Entity, &ApplicationWindow, &CanvasItem, &ChildOf)>()
        .iter(world)
        .filter(|(entity, _, item, parent)| {
            crate::native::topology::previews::demand(world, parent.parent(), *entity, item)
                .is_some()
                && crate::native::topology::presentation::displayed(world, *entity)
                && world
                    .get::<crate::native::topology::presentation::Surface>(*entity)
                    .is_some_and(|surface| surface.visible)
        })
        .map(|(_, application, _, _)| application.surface)
        .collect()
}

pub fn valid_selection(
    world: &World,
    root: Entity,
    workspace: u64,
    item: Option<[u8; 16]>,
) -> bool {
    world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.entries.iter().any(|space| space.id == workspace))
        && !crate::native::laboratory::temporary(world, root, workspace)
        && item.is_none_or(|id| {
            world.get::<Children>(root).is_some_and(|children| {
                children.iter().any(|entity| {
                    world
                        .get::<LayoutBox>(entity)
                        .is_some_and(|layout| layout.id == id)
                        && focus::eligible(world, root, workspace, entity)
                })
            })
        })
}

pub fn activate(world: &mut World, root: Entity, workspace: u64, item: Option<[u8; 16]>) -> bool {
    activate_inner(world, root, workspace, item, true)
}

fn activate_inner(world: &mut World, root: Entity, workspace: u64, item: Option<[u8; 16]>, remember: bool) -> bool {
    if !valid_selection(world, root, workspace, item) {
        crate::native::notifications::report(
            world,
            "Overview",
            "The selected workspace or window is no longer available.",
        );
        return false;
    }
    let previous = remember.then(|| crate::native::workspace::navigation::capture(world, root)).flatten();
    close(world, true);
    crate::native::workspace::switch_output(world, root, workspace);
    if let Some(id) = item {
        focus::ensure(world, root);
        let target = world.get::<Children>(root).unwrap().iter().find(|entity| {
            world
                .get::<LayoutBox>(*entity)
                .is_some_and(|layout| layout.id == id)
        });
        if let Some(target) = target {
            focus::select(world, root, workspace, target, true);
        }
    } else {
        focus::restore_input(world, root);
    }
    if let Some(previous) = previous {
        previous.remember(world);
    }
    true
}

fn navigate(world: &mut World, direction: Direction) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    if let Some(session) = &mut overview.0 {
        if !session.cards.is_empty() {
            let current = session
                .cards
                .iter()
                .position(|card| card.workspace == session.selected.0)
                .unwrap_or(0);
            let next = match direction {
                Direction::Up => current.saturating_sub(1),
                Direction::Down => (current + 1).min(session.cards.len() - 1),
                _ => current,
            };
            let card = &session.cards[next];
            let mut choices = vec![(None, card.title)];
            choices.extend(
                card.order
                    .iter()
                    .filter_map(|id| card.items.get(id).map(|(entity, _)| (Some(*id), *entity))),
            );
            let index = choices
                .iter()
                .position(|(id, _)| *id == session.selected.1)
                .unwrap_or(0);
            let index = if next != current {
                0
            } else {
                match direction {
                    Direction::Left => index.saturating_sub(1),
                    Direction::Right => (index + 1).min(choices.len() - 1),
                    _ => index,
                }
            };
            session.selected = (card.workspace, choices[index].0);
            world
                .resource_mut::<InputFocus>()
                .set(choices[index].1, FocusCause::Navigated);
        }
    }
    world.insert_resource(overview);
}

fn bindings(world: &World) -> (KeyBindings, String) {
    let settings = world
        .get_resource::<Settings>()
        .cloned()
        .unwrap_or_default();
    let mut bindings = vec![
        KeyBinding::new(
            KeyCode::Escape,
            Modifiers::NONE,
            crate::native::actions![Change::Cancel],
        ),
        KeyBinding::new(
            KeyCode::Enter,
            Modifiers::NONE,
            crate::native::actions![Change::Commit],
        ),
    ];
    for (shortcut, arrow, direction) in [
        (Shortcut::FocusLeft, KeyCode::ArrowLeft, Direction::Left),
        (Shortcut::FocusDown, KeyCode::ArrowDown, Direction::Down),
        (Shortcut::FocusUp, KeyCode::ArrowUp, Direction::Up),
        (Shortcut::FocusRight, KeyCode::ArrowRight, Direction::Right),
    ] {
        bindings.push(KeyBinding::new(
            arrow,
            Modifiers::NONE,
            crate::native::actions![Change::Navigate(direction)],
        ));
        if let Some(chord) = settings.chord(shortcut) {
            bindings.push(KeyBinding::new(
                chord.key,
                Modifiers::NONE,
                crate::native::actions![Change::Navigate(direction)],
            ));
        }
    }
    (
        KeyBindings(bindings),
        format!(
            "{} · {}",
            settings.text(Shortcut::Overview),
            crate::i18n::Token::OverviewKeys.text()
        ),
    )
}

pub(crate) fn update(world: &mut World) {
    let Some(mut overview) = world.remove_resource::<Overview>() else {
        return;
    };
    let Some(session) = &mut overview.0 else {
        world.insert_resource(overview);
        return;
    };
    if world.get::<Workspaces>(session.root).is_none()
        || world.get_entity(session.overlay).is_err()
        || output(world, session.root) != session.output
    {
        world.insert_resource(overview);
        close(world, true);
        return;
    }
    let (keys, help) = bindings(world);
    world.entity_mut(session.overlay).insert(keys);
    if world
        .get::<Text>(session.help)
        .is_some_and(|text| text.0 != help)
    {
        world.get_mut::<Text>(session.help).unwrap().0 = help;
    }
    let workspaces: Vec<_> = world
        .get::<Workspaces>(session.root)
        .unwrap()
        .entries
        .iter()
        .filter(|space| !crate::native::laboratory::temporary(world, session.root, space.id))
        .map(|space| (space.id, space.name.clone()))
        .collect();
    let ids: Vec<_> = workspaces.iter().map(|(id, _)| *id).collect();
    if session
        .cards
        .iter()
        .map(|card| card.workspace)
        .collect::<Vec<_>>()
        != ids
    {
        for card in session.cards.drain(..) {
            world.despawn(card.entity);
        }
        for (workspace, name) in &workspaces {
            let entity = world
                .spawn((
                    crate::native::edit_mode::FocusScrollGroup,
                    Node {
                        width: percent(100),
                        min_width: px(0),
                        max_width: px(960),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        ..default()
                    },
                    ChildOf(session.list),
                ))
                .id();
            let title = button(world, entity, name, Change::Select(*workspace, None));
            {
                let mut node = world.get_mut::<Node>(title).unwrap();
                node.min_width = px(0);
                node.overflow = Overflow::clip();
            }
            let caption = world.get::<Children>(title).unwrap()[0];
            world.entity_mut(caption).insert(TextLayout::linebreak(
                bevy::text::LineBreak::WordOrCharacter,
            ));
            {
                let mut node = world.get_mut::<Node>(caption).unwrap();
                node.width = percent(100);
                node.min_width = px(0);
                node.flex_shrink = 1.0;
            }
            let preview = world
                .spawn((
                    Name::new(format!("overview.workspace.{workspace}")),
                    Node {
                        width: percent(100),
                        height: px(180),
                        overflow: Overflow::clip(),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode::default(),
                    ChildOf(entity),
                ))
                .id();
            session.cards.push(Card {
                workspace: *workspace,
                entity,
                title,
                preview,
                image: None,
                items: BTreeMap::new(),
                order: Vec::new(),
            });
        }
    }
    let mut previews = Vec::new();
    let focused = world.resource::<InputFocus>().get();
    for (index, card) in session.cards.iter_mut().enumerate() {
        let spaces = world.get::<Workspaces>(session.root).unwrap();
        let space = spaces
            .entries
            .iter()
            .find(|space| space.id == card.workspace)
            .unwrap();
        let active = spaces.active == card.workspace;
        let selected = space.desktop.focused().map(|id| id.0);
        let monitor = world
            .get_resource::<crate::native::output_views::OutputViews>()
            .filter(|views| views.applies_to(session.root))
            .and_then(|views| {
                views
                    .outputs
                    .iter()
                    .find(|output| output.workspace == card.workspace)
            })
            .map(|output| {
                format!(
                    " · {} {}",
                    crate::i18n::Token::OverviewMonitor.text(),
                    output.id.0
                )
            })
            .unwrap_or_default();
        let current = if active {
            format!(" · {}", crate::i18n::Token::OverviewCurrent.text())
        } else {
            String::new()
        };
        let title = format!("{} · {}{}{}", index + 1, space.name, current, monitor);
        let caption = world.get::<Children>(card.title).unwrap()[0];
        if world
            .get::<Text>(caption)
            .is_some_and(|text| text.0 != title)
        {
            world.get_mut::<Text>(caption).unwrap().0 = title.clone();
            world
                .get_mut::<bevy::a11y::AccessibilityNode>(card.title)
                .unwrap()
                .set_label(title);
        }
        let mut entries: Vec<_> = world
            .get::<Children>(session.root)
            .unwrap()
            .iter()
            .filter_map(|entity| {
                if !focus::eligible(world, session.root, card.workspace, entity) {
                    return None;
                }
                let layout = world.get::<LayoutBox>(entity)?;
                let item = *world.get::<CanvasItem>(entity)?;
                let bounds = crate::native::topology::previews::bounds(
                    world,
                    session.root,
                    card.workspace,
                    entity,
                    &item,
                )?;
                let name = item_name(world, entity);
                Some((layout.id, entity, bounds, name))
            })
            .collect();
        entries.sort_by(|a, b| {
            let center =
                |bounds: (bevy::math::DVec2, bevy::math::DVec2)| (bounds.0 + bounds.1) * 0.5;
            let a_center = center(a.2);
            let b_center = center(b.2);
            a_center
                .x
                .total_cmp(&b_center.x)
                .then_with(|| a_center.y.total_cmp(&b_center.y))
                .then_with(|| a.0.cmp(&b.0))
        });
        card.order = entries.iter().map(|entry| entry.0).collect();
        let alive: BTreeSet<_> = entries.iter().map(|entry| entry.0).collect();
        card.items.retain(|id, (button, _)| {
            if alive.contains(id) {
                true
            } else {
                world.despawn(*button);
                false
            }
        });
        let extent = world
            .get::<ComputedNode>(card.preview)
            .map(ComputedNode::size)
            .filter(|size| size.min_element() > 0.0)
            .unwrap_or(Vec2::new(320.0, 180.0));
        let factor = (768.0 / extent.x).min(432.0 / extent.y).min(1.0);
        let pixels = (extent * factor).ceil().as_uvec2().max(UVec2::ONE);
        let view = crate::native::topology::previews::fit(
            world,
            session.root,
            card.workspace,
            pixels.as_vec2(),
        );
        for (id, _, (min, max), name) in entries {
            let (button, caption) = *card.items.entry(id).or_insert_with(|| {
                let button = button(
                    world,
                    card.preview,
                    &name,
                    Change::Select(card.workspace, Some(id)),
                );
                world
                    .entity_mut(button)
                    .insert(crate::native::icons::Tooltip(name.clone()));
                dragging::install(world, button, session.root, card.workspace, id);
                (button, world.get::<Children>(button).unwrap()[0])
            });
            if world
                .get::<Text>(caption)
                .is_some_and(|text| text.0 != name)
            {
                world.get_mut::<Text>(caption).unwrap().0 = name.clone();
                world
                    .entity_mut(button)
                    .insert(crate::native::icons::Tooltip(name.clone()));
                world
                    .get_mut::<bevy::a11y::AccessibilityNode>(button)
                    .unwrap()
                    .set_label(name);
            }
            let size = (max - min) * view.zoom;
            let origin = (min - view.center) * view.zoom + pixels.as_dvec2() * 0.5;
            world
                .entity_mut(button)
                .insert(crate::native::token_style::border(
                    if selected == Some(id) {
                        crate::native::tokens::Token::Accent
                    } else {
                        crate::native::tokens::Token::SandBorder
                    },
                ));
            world.get_mut::<Node>(button).unwrap().set_if_neq(Node {
                position_type: PositionType::Absolute,
                left: percent((origin.x / f64::from(pixels.x) * 100.0) as f32),
                top: percent((origin.y / f64::from(pixels.y) * 100.0) as f32),
                width: percent((size.x / f64::from(pixels.x) * 100.0) as f32),
                height: percent((size.y / f64::from(pixels.y) * 100.0) as f32),
                border: UiRect::all(px(if selected == Some(id) { 2.0 } else { 1.0 })),
                overflow: Overflow::clip(),
                ..default()
            });
            if !session.focus_selection && focused == Some(button) {
                session.selected = (card.workspace, Some(id));
            }
        }
        if !session.focus_selection && focused == Some(card.title) {
            session.selected = (card.workspace, None);
        }
        let visible = world
            .get::<ComputedNode>(card.preview)
            .zip(world.get::<UiGlobalTransform>(card.preview))
            .is_some_and(|(node, transform)| {
                let center = transform.transform_point2(Vec2::ZERO);
                node.size().min_element() > 0.0
                    && world
                        .get::<bevy::ui::CalculatedClip>(card.preview)
                        .is_none_or(|clip| {
                            clip.clip
                                .intersect(Rect::from_center_size(center, node.size()))
                                .size()
                                .min_element()
                                > 0.0
                        })
            });
        if visible && previews.len() < crate::native::topology::previews::LIMIT {
            if card.image.as_ref().is_none_or(|(size, _)| *size != pixels) {
                let image = world
                    .resource_mut::<Assets<Image>>()
                    .add(Image::new_target_texture(
                        pixels.x,
                        pixels.y,
                        bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb,
                        None,
                    ));
                world.get_mut::<ImageNode>(card.preview).unwrap().image = image.clone();
                card.image = Some((pixels, image));
            }
            previews.push(Preview {
                id: card.preview,
                root: session.root,
                workspace: card.workspace,
                view,
                pixels,
                image: card.image.as_ref().unwrap().1.clone(),
            });
        } else if card.image.take().is_some() {
            world.get_mut::<ImageNode>(card.preview).unwrap().image = Handle::default();
        }
    }
    if !session.cards.iter().any(|card| {
        card.workspace == session.selected.0
            && session
                .selected
                .1
                .is_none_or(|id| card.items.contains_key(&id))
    }) && let Some(card) = session
        .cards
        .iter()
        .find(|card| {
            world
                .get::<Workspaces>(session.root)
                .is_some_and(|spaces| spaces.active == card.workspace)
        })
        .or_else(|| session.cards.first())
    {
        session.selected = (card.workspace, None);
        if focused.is_none_or(|entity| world.get_entity(entity).is_err()) {
            world
                .resource_mut::<InputFocus>()
                .set(card.title, FocusCause::Navigated);
        }
    }
    if session.focus_selection {
        session.focus_selection = false;
        if let Some(card) = session
            .cards
            .iter()
            .find(|card| card.workspace == session.selected.0)
        {
            let target = session
                .selected
                .1
                .and_then(|id| card.items.get(&id).map(|(entity, _)| *entity))
                .unwrap_or(card.title);
            world
                .resource_mut::<InputFocus>()
                .set(target, FocusCause::Navigated);
        }
    }
    let focused = world.resource::<InputFocus>().get();
    let buttons: Vec<_> = world
        .query_filtered::<Entity, With<OverviewButton>>()
        .iter(world)
        .collect();
    for button in buttons {
        let width = px(if focused == Some(button) { 2.0 } else { 0.0 });
        if let Some(mut outline) = world.get_mut::<Outline>(button)
            && outline.width != width
        {
            outline.width = width;
        }
    }
    world.insert_resource(Previews(previews));
    dragging::update(world, session, std::time::Instant::now());
    let activation = hover::update(world, session, std::time::Instant::now());
    world.insert_resource(overview);
    if let Some(request) = activation
        && acknowledge_hover(world, request, true)
    {
        activate_inner(world, Entity::from_bits(request.drag.owner), request.workspace, None, false);
    }
}

#[cfg(test)]
mod tests;
