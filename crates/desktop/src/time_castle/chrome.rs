use super::*;
use crate::actions::{Action, ActionButton};
use crate::icons::{Icon, IconButton, IconStyle};

#[derive(Component)]
pub(super) struct Chrome {
    pub pages: [Entity; 4],
    pub peek: Entity,
    panel: Entity,
    button: Entity,
    center: Entity,
    center_button: Entity,
    tabs: [Entity; 4],
    binding: Option<[Entity; 2]>,
    open: bool,
    tab: usize,
    peek_closed: bool,
    round: bool,
}

pub(super) fn surfaces(world: &World, owner: Entity) -> Vec<Entity> {
    let Some(chrome) = world.get::<Chrome>(owner) else {
        return Vec::new();
    };
    let entry = if chrome.round && !chrome.open {
        chrome.center
    } else {
        chrome.button
    };
    if chrome.open {
        vec![chrome.panel, entry]
    } else if world
        .get::<Node>(chrome.peek)
        .is_some_and(|node| node.display != Display::None)
    {
        vec![entry, chrome.peek]
    } else {
        vec![entry]
    }
}

pub(super) fn controls_open(world: &World, owner: Entity) -> bool {
    world.get::<Chrome>(owner).is_some_and(|chrome| chrome.open)
}

pub(super) fn close_controls(world: &mut World, owner: Entity) {
    if let Some(mut chrome) = world.get_mut::<Chrome>(owner) {
        chrome.open = false;
        sync(world, owner);
    }
}

pub(super) fn presentation(world: &mut World, owner: Entity, round: bool) {
    if let Some(mut chrome) = world.get_mut::<Chrome>(owner)
        && chrome.round != round
    {
        chrome.round = round;
        sync(world, owner);
    }
}

pub(super) fn availability(world: &mut World, owner: Entity, complete: bool) {
    let Some(button) = world.get::<Chrome>(owner).map(|chrome| chrome.button) else {
        return;
    };
    if let Some(mut style) = world.get_mut::<IconStyle>(button) {
        let color = if complete {
            Color::srgb(0.70, 0.83, 0.87)
        } else {
            Color::srgb(0.83, 0.68, 0.44)
        };
        if style.color != color {
            style.color = color;
        }
    }
}

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: impl Action,
) -> Entity {
    let entity = crate::sand_panel::button(world, parent, owner, caption, action);
    world.entity_mut(entity).insert((
        crate::sand::Borderless,
        crate::token_style::background(crate::tokens::Token::Surface),
    ));
    world
        .entity_mut(entity)
        .remove::<crate::token_style::BorderToken>();
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.border = UiRect::ZERO;
    node.border_radius = BorderRadius::all(px(8));
    entity
}

pub(super) fn populate(world: &mut World, owner: Entity) {
    let panel = world
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(10),
                right: px(10),
                top: px(10),
                bottom: px(10),
                padding: UiRect::all(px(18)),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                border_radius: BorderRadius::all(px(20)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.065, 0.095, 0.12)),
            ZIndex(20),
            ChildOf(owner),
        ))
        .id();
    crate::edit_mode::label(world, panel, "Time Castle", 18.0);
    let row = crate::sand_panel::row(world, panel);
    let tabs = ["Clock", "Agenda", "Stopwatch", "Sound"].map(|caption| {
        let tab = match caption {
            "Clock" => 0,
            "Agenda" => 1,
            "Stopwatch" => 2,
            _ => 3,
        };
        button(world, row, owner, caption, Tab(tab))
    });
    let pages = std::array::from_fn(|index| {
        world
            .spawn((
                Node {
                    display: if index == 0 {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    width: percent(100),
                    min_height: px(0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    overflow: if index == 2 {
                        Overflow::clip()
                    } else {
                        Overflow::scroll_y()
                    },
                    ..default()
                },
                ScrollPosition::default(),
                ChildOf(panel),
            ))
            .id()
    });
    for (index, page) in pages.iter().enumerate() {
        if index != 2 {
            crate::scroll_sand::attach(world, *page);
        }
    }
    world.spawn((
        Node {
            height: px(72),
            min_height: px(72),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(pages[2]),
    ));
    let corner = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(14),
                top: px(14),
                ..default()
            },
            ZIndex(40),
            ChildOf(owner),
        ))
        .id();
    let button = world
        .spawn((
            IconButton::new(Icon::General, "Clock controls"),
            IconStyle {
                size: 16.0,
                padding: 6.0,
                radius: 32.0,
                color: Color::srgb(0.70, 0.83, 0.87),
                background: Color::srgb(0.075, 0.12, 0.15),
                border_color: Color::srgb(0.22, 0.32, 0.38),
            },
            crate::sand::Borderless,
            crate::tokens::TokenOverrides(std::collections::BTreeMap::from([(
                crate::tokens::Token::ControlRoundness,
                crate::tokens::TokenValue::Number(32.0),
            )])),
            ActionButton::new(owner, crate::actions![ToggleControls]),
            ChildOf(corner),
        ))
        .id();
    let center = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(75),
                margin: UiRect {
                    left: px(-16),
                    top: px(-16),
                    ..default()
                },
                width: px(32),
                height: px(32),
                ..default()
            },
            ZIndex(40),
            ChildOf(owner),
        ))
        .id();
    let mut skull_accessibility = bevy::a11y::AccessibilityNode::default();
    skull_accessibility.set_label("Memento mori. Clock controls");
    let center_button = world
        .spawn((
            crate::sand::button(0),
            Node {
                width: px(32),
                height: px(32),
                padding: UiRect::ZERO,
                ..default()
            },
            BackgroundColor(Color::NONE),
            crate::icons::Tooltip("Clock controls".into()),
            skull_accessibility,
            crate::sand::Borderless,
            ActionButton::new(owner, crate::actions![ToggleControls]),
            ChildOf(center),
        ))
        .id();
    let peek = world
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: percent(18),
                top: percent(40),
                width: percent(64),
                max_height: percent(45),
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                border_radius: BorderRadius::all(px(14)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            BackgroundColor(Color::srgb(0.065, 0.095, 0.12)),
            ZIndex(10),
            ChildOf(owner),
        ))
        .id();
    crate::scroll_sand::attach(world, peek);
    world.entity_mut(owner).insert(Chrome {
        pages,
        peek,
        panel,
        button,
        center,
        center_button,
        tabs,
        binding: None,
        open: false,
        tab: 0,
        peek_closed: false,
        round: true,
    });
    sync(world, owner);
}

pub(super) fn stopwatch(world: &mut World, owner: Entity, input: Entity) -> Entity {
    let caption = crate::edit_mode::label(world, owner, "Stopwatch Record · optional", 12.0);
    world.entity_mut(caption).insert((
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: px(28),
            top: px(104),
            ..default()
        },
        ZIndex(30),
    ));
    if let Some(mut node) = world.get_mut::<Node>(input) {
        node.display = Display::None;
        node.position_type = PositionType::Absolute;
        node.left = px(28);
        node.top = px(126);
        node.width = percent(85);
        node.height = px(32);
    }
    if let Some(mut area) = world.get_mut::<crate::sand_text::SandText>(input) {
        area.offset = [28.0, 126.0];
        area.size = [344.0, 32.0];
    }
    world.entity_mut(input).insert(ZIndex(30));
    let mut chrome = world.get_mut::<Chrome>(owner).unwrap();
    chrome.binding = Some([caption, input]);
    chrome.pages[2]
}

pub(super) fn colors(world: &mut World, owner: Entity, palette: &palette::Palette) {
    let Some(chrome) = world.get::<Chrome>(owner) else {
        return;
    };
    let (panel, peek, tabs, button, center, tab) = (
        chrome.panel,
        chrome.peek,
        chrome.tabs,
        chrome.button,
        chrome.center_button,
        chrome.tab,
    );
    for entity in [panel, peek] {
        world
            .get_mut::<BackgroundColor>(entity)
            .unwrap()
            .set_if_neq(BackgroundColor(palette.surface));
    }
    for (index, entity) in tabs.into_iter().enumerate() {
        let color = if index == tab {
            palette.event.with_alpha(0.2)
        } else {
            palette.track.with_alpha(0.25)
        };
        world
            .get_mut::<BackgroundColor>(entity)
            .unwrap()
            .set_if_neq(BackgroundColor(color));
    }
    let cursor = world.get::<TimeSettings>(owner).unwrap().0.cursor;
    let choices: Vec<_> = world
        .query::<(Entity, &ui::CursorChoice, &ActionButton)>()
        .iter(world)
        .filter(|(_, _, action)| action.target == owner)
        .map(|(entity, choice, _)| (entity, choice.0 == cursor))
        .collect();
    for (entity, selected) in choices {
        let color = if selected {
            palette.event.with_alpha(0.25)
        } else {
            palette.track.with_alpha(0.15)
        };
        world
            .get_mut::<BackgroundColor>(entity)
            .unwrap()
            .set_if_neq(BackgroundColor(color));
    }
    let color = if world.get::<View>(owner).is_some_and(|view| {
        !matches!(
            view.projection,
            Some(nucleus::projection::Status::Ready { .. })
        )
    }) {
        palette.present
    } else {
        palette.ink
    };
    for button in [button, center] {
        let Some(mut style) = world.get_mut::<IconStyle>(button) else {
            continue;
        };
        if style.color != color {
            style.color = color;
        }
        if style.background != palette.surface {
            style.background = palette.surface;
        }
        if style.border_color != palette.track {
            style.border_color = palette.track;
        }
    }
}

fn sync(world: &mut World, owner: Entity) {
    let chrome = world.get::<Chrome>(owner).unwrap();
    let (panel, button, pages, tabs, binding, open, tab, peek, peek_closed) = (
        chrome.panel,
        chrome.button,
        chrome.pages,
        chrome.tabs,
        chrome.binding,
        chrome.open,
        chrome.tab,
        chrome.peek,
        chrome.peek_closed,
    );
    world.get_mut::<Node>(panel).unwrap().display =
        if open { Display::Flex } else { Display::None };
    let corner = world.get::<ChildOf>(button).unwrap().parent();
    let (center, round) = {
        let chrome = world.get::<Chrome>(owner).unwrap();
        (chrome.center, chrome.round)
    };
    world.get_mut::<Node>(center).unwrap().display = if round && !open {
        Display::Flex
    } else {
        Display::None
    };
    let mut node = world.get_mut::<Node>(corner).unwrap();
    node.display = if round && !open {
        Display::None
    } else {
        Display::Flex
    };
    node.right = if open { px(14) } else { Val::Auto };
    node.left = if open { Val::Auto } else { percent(50) };
    node.top = if open { px(14) } else { percent(72) };
    node.margin.left = if open { px(0) } else { px(-14) };
    for (index, page) in pages.into_iter().enumerate() {
        world.get_mut::<Node>(page).unwrap().display = if index == tab {
            Display::Flex
        } else {
            Display::None
        };
        world.get_mut::<BackgroundColor>(tabs[index]).unwrap().0 = if index == tab {
            Color::srgba(0.28, 0.49, 0.50, 0.45)
        } else {
            Color::srgba(0.16, 0.23, 0.28, 0.55)
        };
    }
    if let Some(binding) = binding {
        for entity in binding {
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                node.display = if open && tab == 2 {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
    }
    let mut icon = world.get_mut::<IconButton>(button).unwrap();
    icon.icon = if open { Icon::Close } else { Icon::General };
    icon.label = if open {
        "Hide clock controls"
    } else {
        "Clock controls"
    }
    .into();
    let selected = world
        .get::<View>(owner)
        .is_some_and(|view| !view.selected.is_empty());
    world.get_mut::<Node>(peek).unwrap().display = if selected && !open && !peek_closed && !round {
        Display::Flex
    } else {
        Display::None
    };
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.detail_revision = u64::MAX;
    }
    let mut focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    while let Some(entity) = focused {
        if entity == owner {
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .clear();
            break;
        }
        focused = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(super) fn reveal(world: &mut World, owner: Entity) {
    if let Some(mut chrome) = world.get_mut::<Chrome>(owner) {
        chrome.peek_closed = false;
    }
}

#[derive(Clone)]
struct DismissPeek;
impl Action for DismissPeek {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut chrome) = world.get_mut::<Chrome>(owner) {
            chrome.peek_closed = true;
            let peek = chrome.peek;
            world.get_mut::<Node>(peek).unwrap().display = Display::None;
        }
    }
}

#[derive(Clone)]
struct ToggleControls;
impl Action for ToggleControls {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut chrome) = world.get_mut::<Chrome>(owner) {
            chrome.open = !chrome.open;
            sync(world, owner);
        }
    }
}

#[derive(Clone)]
struct Tab(usize);
impl Action for Tab {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut chrome) = world.get_mut::<Chrome>(owner) {
            chrome.tab = self.0;
            sync(world, owner);
        }
    }
}

pub(super) fn dismiss(world: &mut World) {
    if !world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
    {
        return;
    }
    let owners: Vec<_> = world
        .query::<(Entity, &Chrome)>()
        .iter(world)
        .filter(|(_, chrome)| chrome.open)
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        world.get_mut::<Chrome>(owner).unwrap().open = false;
        sync(world, owner);
    }
    let owners: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| !view.selected.is_empty())
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        ui::Select(Vec::new()).apply(world, owner);
    }
}

pub(super) fn peek(world: &mut World, owner: Entity, entries: &[Entry], settings: &Settings) {
    let Some(chrome) = world.get::<Chrome>(owner) else {
        return;
    };
    let (parent, open, closed, round) =
        (chrome.peek, chrome.open, chrome.peek_closed, chrome.round);
    world.entity_mut(parent).despawn_children();
    world.get_mut::<Node>(parent).unwrap().display =
        if !entries.is_empty() && !open && !closed && !round {
            Display::Flex
        } else {
            Display::None
        };
    if entries.is_empty() || round {
        return;
    }
    let row = crate::sand_panel::row(world, parent);
    let title = if entries.len() == 1 {
        "Selected event".into()
    } else {
        format!("{} overlapping events", entries.len())
    };
    let heading = crate::edit_mode::label(world, row, &title, 12.0);
    world.get_mut::<Node>(heading).unwrap().flex_grow = 1.0;
    button(world, row, owner, "Close", DismissPeek);
    let pages = entries.len().div_ceil(20).max(1);
    let page = world
        .get::<View>(owner)
        .map_or(0, |view| view.page)
        .min(pages - 1);
    for entry in entries.iter().skip(page * 20).take(20) {
        let time = ui::time_label(entry, settings, chrono::Utc::now().timestamp_millis());
        let event = button(
            world,
            parent,
            owner,
            &format!("{}\n{time}", ui::headline(&entry.head)),
            ui::Select(vec![entry.id.clone()]),
        );
        world.get_mut::<Node>(event).unwrap().width = percent(100);
    }
    if pages > 1 {
        let row = crate::sand_panel::row(world, parent);
        button(world, row, owner, "Previous", ui::Page(-1));
        button(world, row, owner, "Next", ui::Page(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((
                Node::default(),
                crate::canvas::CanvasItem {
                    size: Vec2::splat(420.0),
                    position: bevy::math::DVec2::ZERO,
                },
            ))
            .id();
        let input = crate::sand_text::spawn(
            &mut world,
            owner,
            crate::sand_text::SavedText {
                area: crate::sand_text::SandText::new(true),
                text: String::new(),
            },
        );
        crate::work_timer::populate(
            &mut world,
            owner,
            None,
            &serde_json::Value::Null,
            Some(input),
        );
        (world, owner, input)
    }

    fn visible(world: &World, mut entity: Entity) -> bool {
        loop {
            if world
                .get::<Node>(entity)
                .is_some_and(|node| node.display == Display::None)
            {
                return false;
            }
            let Some(parent) = world.get::<ChildOf>(entity) else {
                return true;
            };
            entity = parent.parent();
        }
    }

    #[test]
    fn round_clock_has_only_its_center_entry_and_keeps_selection_outside_configuration() {
        let (mut world, owner, _) = fixture();
        let chrome = world.get::<Chrome>(owner).unwrap();
        let (center, center_button, button, peek_entity) = (
            chrome.center,
            chrome.center_button,
            chrome.button,
            chrome.peek,
        );
        assert!(visible(&world, center));
        assert!(!visible(&world, button));
        assert_eq!(surfaces(&world, owner), [center]);
        assert_eq!(world.get::<BackgroundColor>(center).unwrap().0, Color::NONE);
        assert!(world.get::<IconButton>(center_button).is_none());
        assert_eq!(
            world.get::<crate::icons::Tooltip>(center_button).unwrap().0,
            "Clock controls"
        );
        assert_eq!(
            world.get::<ChildOf>(center_button).unwrap().parent(),
            center
        );
        assert_eq!(world.get::<Node>(center).unwrap().left, percent(50));
        assert_eq!(world.get::<Node>(center).unwrap().top, percent(75));
        presentation(&mut world, owner, false);
        assert!(!visible(&world, center));
        assert!(visible(&world, button));
        presentation(&mut world, owner, true);
        peek(&mut world, owner, &[], &Settings::default());
        assert!(!visible(&world, peek_entity));
        assert!(visible(&world, center));
        ToggleControls.apply(&mut world, owner);
        assert!(!visible(&world, center));
        ToggleControls.apply(&mut world, owner);
        assert!(visible(&world, center));
        assert!(!visible(&world, button));
    }

    #[test]
    fn hiding_clock_controls_preserves_drafts_and_stopwatch_storage() {
        let (mut world, owner, input) = fixture();
        let chrome = world.get::<Chrome>(owner).unwrap();
        let (panel, button) = (chrome.panel, chrome.button);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::None);
        let actions: Vec<_> = world
            .query_filtered::<Entity, With<ActionButton>>()
            .iter(&world)
            .collect();
        assert_eq!(
            actions
                .into_iter()
                .filter(|entity| visible(&world, *entity))
                .count(),
            1
        );
        let aperture = world.get::<View>(owner).unwrap().fields[0];
        world
            .get_mut::<bevy::text::EditableText>(aperture)
            .unwrap()
            .editor
            .set_text("120");
        world
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .editor
            .set_text("draft-record");
        let running: crate::work_timer::LocalTimer =
            serde_json::from_value(serde_json::json!({"logs":[
                {"id":"work.log:running","start":"2026-10-03T14:30:00Z","end":null}
            ]}))
            .unwrap();
        world.entity_mut(owner).insert(running.clone());
        ToggleControls.apply(&mut world, owner);
        assert_eq!(
            world.get::<IconButton>(button).unwrap().label,
            "Hide clock controls"
        );
        Tab(2).apply(&mut world, owner);
        assert!(visible(&world, input));
        ToggleControls.apply(&mut world, owner);
        assert!(!visible(&world, input));
        assert_eq!(crate::sand_text::value(&world, aperture), "120");
        assert_eq!(
            crate::sand_text::snapshot(&world, owner)[0].text,
            "draft-record"
        );
        assert_eq!(
            world.get::<crate::work_timer::LocalTimer>(owner).unwrap(),
            &running
        );
        assert_eq!(
            world.get::<TimeSettings>(owner).unwrap().0.aperture_ms,
            3_600_000
        );
        ToggleControls.apply(&mut world, owner);
        assert!(visible(&world, input));
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Escape);
        world.insert_resource(keys);
        dismiss(&mut world);
        assert!(!visible(&world, input));
        assert_eq!(
            world.get::<IconButton>(button).unwrap().label,
            "Clock controls"
        );
    }

    #[test]
    fn event_details_are_bounded_and_dismiss_without_losing_selection() {
        let (mut world, owner, _) = fixture();
        presentation(&mut world, owner, false);
        let now = chrono::Utc::now().timestamp_millis();
        let entries: Vec<_> = (0..50)
            .map(|index| Entry {
                id: format!("event-{index}"),
                record_uid: nucleus::new_uid("r"),
                head: format!("Event {index}"),
                quantity: "-1".into(),
                category: model::Category::Timed,
                time: Some(nucleus::schedule::TimeRange {
                    from_ms: now + 60_000,
                    until_ms: None,
                }),
                origin: serde_json::json!({"kind":"manual"}),
                preview: false,
                start_date: None,
                due_date: None,
            })
            .collect();
        let ids = entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        world.get_mut::<View>(owner).unwrap().entries = entries;
        ui::Select(ids.clone()).apply(&mut world, owner);
        ui::details(&mut world, owner, now);
        let peek = world.get::<Chrome>(owner).unwrap().peek;
        assert!(visible(&world, peek));
        let actions: Vec<_> = world
            .query_filtered::<Entity, With<ActionButton>>()
            .iter(&world)
            .collect();
        assert_eq!(
            actions
                .into_iter()
                .filter(|entity| visible(&world, *entity))
                .count(),
            24
        );
        DismissPeek.apply(&mut world, owner);
        assert!(!visible(&world, peek));
        assert_eq!(world.get::<View>(owner).unwrap().selected, ids);
        ui::Select(vec![ids[30].clone()]).apply(&mut world, owner);
        ui::details(&mut world, owner, now);
        assert!(visible(&world, peek));
        assert_eq!(
            world.get::<View>(owner).unwrap().selected,
            [ids[30].clone()]
        );
    }
}
