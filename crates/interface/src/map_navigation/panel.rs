use super::{Panel, RouteEdit, operate, requests::Lookup};
use crate::{
    controls,
    style::{background, border, text},
    theme::Typography,
    tokens::Token,
};
use bevy::{a11y::AccessibilityNode, prelude::*, ui_widgets::Activate};
use lince_map::{
    Coordinate,
    navigation::{Mode, Provider, Selection, Settings},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::runtime::Handle;

#[derive(Clone, Copy)]
pub(super) enum Intent {
    Open,
    ToggleSettings,
    Save,
    Choose(usize),
    EditRoute(RouteEdit),
    Request(Lookup),
    PrepareRegion,
}

pub(super) fn toggle(world: &mut World, entity: Entity) -> Option<Display> {
    let mut node = world.get_mut::<Node>(entity)?;
    node.display = if node.display == Display::None {
        Display::Flex
    } else {
        Display::None
    };
    Some(node.display)
}

#[derive(Component, Clone, Copy)]
pub(super) struct Button {
    panel: Entity,
    intent: Intent,
}

fn label(world: &mut World, parent: Entity, caption: impl Into<String>) -> Entity {
    let font = world.resource::<Typography>().text(14.0);
    world
        .spawn((ChildOf(parent), Text::new(caption), font, text(Token::Ink)))
        .id()
}
pub(super) fn button(
    world: &mut World,
    parent: Entity,
    panel: Entity,
    caption: &str,
    intent: Intent,
) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Node {
                min_height: px(44),
                padding: UiRect::axes(px(8), px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            border(Token::Accent),
            Button { panel, intent },
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<AccessibilityNode>(entity) {
        accessibility.set_label(caption);
    }
    label(world, entity, caption);
    world.entity_mut(entity).observe(
        |activate: On<Activate>, buttons: Query<&Button>, mut commands: Commands| {
            if let Ok(button) = buttons.get(activate.entity).copied() {
                commands.queue(move |world: &mut World| apply(world, button.panel, button.intent));
            }
        },
    );
}
fn field(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    label(world, parent, caption);
    let editor = controls::single_line_editor(value, world.resource::<Typography>(), 0, 4096);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(caption);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            editor,
            controls::Editor {
                title: caption.into(),
                secret: false,
            },
            AccessibilityNode::from(accessibility),
        ))
        .id()
}

pub fn mount(
    world: &mut World,
    controls_parent: Entity,
    owner: Entity,
    path: PathBuf,
    point: Coordinate,
    runtime: Option<Handle>,
) {
    let settings = Settings::load(&path).unwrap_or_default();
    let panel = world
        .spawn((
            ChildOf(owner),
            Node {
                position_type: PositionType::Absolute,
                top: px(42),
                width: percent(100),
                max_height: percent(85),
                padding: UiRect::all(px(8)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            ZIndex(30),
            ScrollPosition::default(),
            background(Token::Surface),
        ))
        .id();
    button(
        world,
        controls_parent,
        panel,
        "Search / directions",
        Intent::Open,
    );
    button(
        world,
        panel,
        panel,
        "Close search / directions",
        Intent::Open,
    );
    label(
        world,
        panel,
        "Search changes nothing until you choose and save a place. Route endpoints use the visible map point.",
    );
    let query = field(world, panel, "Place search", "");
    let configuration = world
        .spawn((
            ChildOf(panel),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
        ))
        .id();
    let output = path.parent().unwrap_or(Path::new("maps")).join("my-region");
    let configuration_fields = [
        (
            "Local place index JSON",
            settings.places.to_string_lossy().as_ref(),
        ),
        ("Geocoding base URL (Nominatim)", &settings.geocoder),
        ("Routing base URL (OSRM)", &settings.router),
        ("Provider/data credits", &settings.credits),
        (
            "Allow provider requests (true/false)",
            if settings.online { "true" } else { "false" },
        ),
        ("Regional dataset name", "my-region"),
        ("Regional data date", ""),
        ("Regional original source", ""),
        ("Regional data license", ""),
        ("Regional credits", ""),
        (
            "New regional output directory",
            output.to_string_lossy().as_ref(),
        ),
        ("Regional minimum zoom", "0"),
        ("Regional maximum zoom", "14"),
        (
            "Minimum provider request interval (milliseconds)",
            &settings.minimum_interval_ms.to_string(),
        ),
    ]
    .map(|(caption, value)| field(world, configuration, caption, value));
    use Intent::{EditRoute, PrepareRegion, Request, Save, ToggleSettings};
    use Lookup::{Reverse, Route, Search};
    use RouteEdit::{Cancel, CycleMode, Destination, Origin};
    let row = controls::row(world, panel);
    for (caption, intent) in [
        ("Providers / regional packs", ToggleSettings),
        ("Save providers", Save),
        ("Search", Request(Search)),
        ("Name map point", Request(Reverse)),
        ("Use point as start", EditRoute(Origin)),
        ("Use point as destination", EditRoute(Destination)),
        ("Travel mode", EditRoute(CycleMode)),
        ("Find route", Request(Route)),
        ("Prepare selected region", PrepareRegion),
        ("Cancel / clear", EditRoute(Cancel)),
    ] {
        button(world, row, panel, caption, intent);
    }
    let status = label(world, panel, "");
    let results = world
        .spawn((
            ChildOf(panel),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
        ))
        .id();
    let provider = Provider::new(settings).ok().map(Arc::new);
    world.entity_mut(panel).insert(Panel {
        owner,
        path,
        provider,
        runtime,
        point,
        origin: None,
        destination: None,
        mode: Mode::Driving,
        selection: Selection::default(),
        query,
        configuration_fields,
        results,
        status,
        configuration,
        job: None,
        selected: None,
        changed: false,
        region: None,
        scope: String::new(),
    });
}

fn apply(world: &mut World, panel: Entity, intent: Intent) {
    let result = operate(world, panel, intent);
    if let Err(error) = result
        && let Some(state) = world.get::<Panel>(panel)
    {
        controls::status(world, state.status, error);
    }
}
