use crate::{
    app::{self, Mobile},
    navigation::Page,
};
use bevy::prelude::*;
use lince_interface::{
    controls,
    transfers::{
        self as model,
        workspace::{Panel, Workflow, Workspace},
    },
};
use serde_json::Value;

#[derive(Resource, Default)]
struct State {
    workspace: Workspace,
    tools: bool,
}

pub(crate) fn restore(world: &mut World) {
    world.init_resource::<State>();
    let scope = scope_key(world);
    if world.resource::<State>().workspace.matches_scope(&scope) {
        return;
    }
    let restored = world
        .resource::<Mobile>()
        .drafts
        .get("transport-ui/current")
        .filter(|raw| raw.len() <= 16_384)
        .and_then(|raw| serde_json::from_str::<Workspace>(raw).ok())
        .filter(|workspace| workspace.valid() && workspace.matches_scope(&scope));
    let mut workspace = restored.unwrap_or_default();
    workspace.scope(&scope);
    world.insert_resource(State {
        workspace,
        tools: false,
    });
}

fn persist(world: &mut World) {
    if let Ok(value) = serde_json::to_string(&world.resource::<State>().workspace) {
        let mut mobile = world.resource_mut::<Mobile>();
        if mobile.drafts.get("transport-ui/current") != Some(&value) {
            mobile.drafts.insert("transport-ui/current".into(), value);
            mobile.request_save();
        }
    }
}

#[derive(Clone)]
pub enum Intent {
    Enter,
    Back,
    Panel(Panel),
    Select(String),
    History,
    Page(isize),
    Stop,
}

pub(crate) fn scope_key(world: &World) -> String {
    format!(
        "{}:{}",
        world.resource::<Mobile>().scope_key(),
        crate::workflows::person(world)
    )
}

pub(crate) fn active(world: &World) -> bool {
    world.get_resource::<State>().is_some_and(|state| {
        state.workspace.workflow == Workflow::Transport
            && state.workspace.matches_scope(&scope_key(world))
            && matches!(
                world.resource::<Mobile>().navigation.current,
                Page::Transfers | Page::Transfer(_)
            )
            && !world.resource::<Mobile>().navigation.menu_open
    })
}

pub(crate) fn map_tools(world: &World) -> bool {
    world
        .get_resource::<State>()
        .is_some_and(|state| state.workspace.panel == Panel::Map)
}

pub(crate) fn page_key(world: &World) -> String {
    let workspace = &world.resource::<State>().workspace;
    format!("Transport:{:?}:{}", workspace.panel, workspace.selected)
}

pub(crate) fn apply(world: &mut World, intent: Intent) -> Result<(), String> {
    world.init_resource::<State>();
    app::capture(world);
    let scope = scope_key(world);
    world.resource_mut::<State>().workspace.scope(&scope);
    if !matches!(intent, Intent::Enter | Intent::Stop)
        && world
            .get_resource::<crate::workflows::State>()
            .is_some_and(|state| state.form.is_some())
    {
        crate::workflows::persist_draft(world);
        return Err("Your draft is kept. Save or cancel it before leaving.".into());
    }
    match intent {
        Intent::Enter => {
            let selection = match &world.resource::<Mobile>().navigation.current {
                Page::Transfer(uid) => uid.clone(),
                _ => String::new(),
            };
            world
                .resource_mut::<State>()
                .workspace
                .enter(&scope, &selection);
            world
                .resource_mut::<Mobile>()
                .navigation
                .open(Page::Transfers);
        }
        Intent::Back => {
            if let Some(selection) = world.resource_mut::<State>().workspace.back() {
                let page = if selection.is_empty() {
                    Page::Transfers
                } else {
                    Page::Transfer(selection)
                };
                world.resource_mut::<Mobile>().navigation.open(page);
            }
        }
        Intent::Panel(panel) => world.resource_mut::<State>().workspace.panel = panel,
        Intent::Select(uid) => {
            world.resource_mut::<State>().workspace.select(&uid);
            world
                .resource_mut::<Mobile>()
                .navigation
                .open(Page::Transfer(uid));
        }
        Intent::History => {
            let mut state = world.resource_mut::<State>();
            state.workspace.history = !state.workspace.history;
            state.workspace.panel = Panel::List;
            state.workspace.page = 0;
        }
        Intent::Page(delta) => {
            let mut state = world.resource_mut::<State>();
            state.workspace.page = state.workspace.page.saturating_add_signed(delta);
        }
        Intent::Stop => {
            if let Some(row) = selected(world) {
                let person = crate::workflows::person(world);
                app::act(
                    world,
                    engine::actions::Action::Location {
                        request: nucleus::location::Command::Stop {
                            record_uid: model::transport::sharing_record(&row, &person),
                            person,
                        },
                    },
                    None,
                )?;
            }
        }
    }
    let tools = map_tools(world);
    if tools != world.resource::<State>().tools {
        world.resource_mut::<State>().tools = tools;
        crate::map::close_transport_surface(world);
    }
    persist(world);
    app::subscriptions(world)?;
    world.resource_mut::<Mobile>().dirty = true;
    Ok(())
}

fn selected(world: &World) -> Option<Value> {
    let selected = &world.resource::<State>().workspace.selected;
    world
        .resource::<Mobile>()
        .rows
        .get("transfer")
        .into_iter()
        .flatten()
        .chain(
            world
                .resource::<Mobile>()
                .rows
                .get("transport")
                .into_iter()
                .flatten(),
        )
        .find(|row| model::text(row, "uid") == *selected && model::workspace::is_transport(row))
        .cloned()
}

fn button(world: &mut World, parent: Entity, label: &str, intent: Intent) {
    app::button(world, parent, label, app::Intent::Transport(intent));
}

pub(crate) fn render(world: &mut World, parent: Entity) {
    crate::workflows::ensure_scope(world);
    let workspace = world.resource::<State>().workspace.clone();
    let rows = world
        .resource::<Mobile>()
        .rows
        .get("transport")
        .cloned()
        .unwrap_or_default();
    let person = crate::workflows::person(world);
    let visible = workspace.rows(&rows, &person);
    let current = match &world.resource::<Mobile>().navigation.current {
        Page::Transfer(uid) => rows
            .iter()
            .find(|row| model::text(row, "uid") == *uid)
            .cloned(),
        _ => None,
    };
    let row = current
        .or_else(|| {
            visible
                .iter()
                .find(|row| model::text(row, "uid") == workspace.selected)
                .map(|row| (*row).clone())
        })
        .or_else(|| visible.first().map(|row| (*row).clone()));
    if let Some(row) = &row {
        world.resource_mut::<State>().workspace.selected = model::text(row, "uid");
    }
    persist(world);
    crate::map::transport(
        world,
        row.as_ref().map(Workspace::records).unwrap_or_default(),
    );
    crate::map::render(world, parent);
    let header = controls::row(world, parent);
    world.entity_mut(header).insert((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            right: px(8),
            top: px(8),
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(8),
            ..default()
        },
        GlobalZIndex(60),
    ));
    button(world, header, "Back", Intent::Back);
    button(
        world,
        header,
        "Transport options",
        Intent::Panel(Panel::Options),
    );
    if workspace.panel == Panel::Map {
        return;
    }
    let editor = world.resource::<crate::workflows::State>().form.is_some();
    let panel = controls::column(world, parent);
    world.entity_mut(panel).insert((
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            bottom: px(0),
            top: if editor { px(0) } else { Val::Auto },
            max_height: if editor {
                percent(100)
            } else if workspace.panel == Panel::Summary {
                percent(40)
            } else {
                percent(85)
            },
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(16)),
            row_gap: px(10),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        GlobalZIndex(60),
        lince_interface::style::background(lince_interface::tokens::Token::Surface),
    ));
    if editor {
        crate::workflows::render(world, panel);
        return;
    }
    let status = world.resource::<Mobile>().status.clone();
    let status = app::label(world, panel, &status, 14.0);
    world.entity_mut(status).insert(app::Status);
    if workspace.panel == Panel::Summary {
        button(
            world,
            panel,
            "My active transports",
            Intent::Panel(Panel::List),
        );
    } else {
        button(world, panel, "Collapse", Intent::Panel(Panel::Summary));
    }
    match workspace.panel {
        Panel::Summary => {
            if let Some(row) = &row {
                app::label(world, panel, &model::title(row), 22.0);
                app::label(world, panel, &model::human(&model::status(row)), 15.0);
                if let Some(form) = model::transport::journey_forms(row, &person)
                    .into_iter()
                    .next()
                {
                    app::button(
                        world,
                        panel,
                        &form.title.clone(),
                        app::Intent::Workflow(crate::workflows::Intent::Open(form)),
                    );
                } else {
                    button(
                        world,
                        panel,
                        "View transport",
                        Intent::Select(model::text(row, "uid")),
                    );
                }
                button(world, panel, "Stop my sharing", Intent::Stop);
            } else {
                app::label(world, panel, "No active transports", 22.0);
                new(world, panel, "Ride");
            }
        }
        Panel::List => {
            let page = workspace.page.min(visible.len().saturating_sub(1) / 30);
            world.resource_mut::<State>().workspace.page = page;
            for row in visible.iter().skip(page * 30).take(30) {
                button(
                    world,
                    panel,
                    &model::title(row),
                    Intent::Select(model::text(row, "uid")),
                );
            }
            if page > 0 {
                button(world, panel, "Previous transports", Intent::Page(-1));
            }
            if (page + 1) * 30 < visible.len() {
                button(world, panel, "More transports", Intent::Page(1));
            }
            new(world, panel, "Ride");
        }
        Panel::Options => {
            if !matches!(
                world.resource::<Mobile>().identity,
                crate::session::Identity::Person(_)
            ) {
                app::input(
                    world,
                    panel,
                    "workflow",
                    "person",
                    "Acting Person Record",
                    &person,
                    false,
                );
            }
            new(world, panel, "Ride");
            new(world, panel, "Delivery");
            button(
                world,
                panel,
                if workspace.history {
                    "Show my active transports"
                } else {
                    "Find transports and history"
                },
                Intent::History,
            );
            button(
                world,
                panel,
                "Map settings and navigation",
                Intent::Panel(Panel::Map),
            );
            if let Some(row) = &row {
                button(
                    world,
                    panel,
                    "All transport actions",
                    Intent::Select(model::text(row, "uid")),
                );
                if model::capability(row, "edit_terms") {
                    app::button(
                        world,
                        panel,
                        "Edit transport",
                        app::Intent::Workflow(crate::workflows::Intent::Open(model::edit(
                            row, false, &person,
                        ))),
                    );
                }
            }
        }
        Panel::Details => {
            if let Some(row) = &row {
                if world.resource::<Mobile>().navigation.current
                    != Page::Transfer(model::text(row, "uid"))
                {
                    button(
                        world,
                        panel,
                        "Open transport",
                        Intent::Select(model::text(row, "uid")),
                    );
                } else {
                    crate::workflows::render(world, panel);
                }
            }
        }
        Panel::Map => {}
    }
    app::label(
        world,
        panel,
        "Natural Earth · © OpenStreetMap contributors · ESA WorldCover",
        11.0,
    );
}

fn new(world: &mut World, parent: Entity, preset: &str) {
    app::button(
        world,
        parent,
        &format!("New {}", preset.to_lowercase()),
        app::Intent::Workflow(crate::workflows::Intent::New(preset.into())),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn panels_keep_one_map_and_back_preserves_an_open_draft() {
        let directory = tempfile::tempdir().unwrap();
        let person = nucleus::new_uid("r");
        let trip = nucleus::new_uid("r");
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<lince_interface::theme::Typography>();
        world.init_resource::<bevy::input_focus::InputFocus>();
        world.init_resource::<lince_interface::tokens::ThemeSettings>();
        let mut mobile = Mobile::new(directory.path().into());
        mobile.ready = true;
        mobile.identity = crate::session::Identity::Person(person.clone());
        mobile.navigation.open(Page::Transfers);
        mobile.rows.insert("transport".into(), vec![
            json!({"uid":trip,"head":"My active trip","primary_status":"active","parties":[{"actor":person}],"promises":[{"transport":{}}]}),
            json!({"uid":nucleus::new_uid("r"),"head":"Another person's trip","primary_status":"active","parties":[{"actor":"other"}],"promises":[{"transport":{}}]}),
        ]);
        world.insert_resource(mobile);
        let mut state = State::default();
        state.workspace.enter(&scope_key(&world), "");
        world.insert_resource(state);
        let parent = world.spawn(Node::default()).id();
        for panel in [Panel::Summary, Panel::List, Panel::Options, Panel::Summary] {
            controls::clear(&mut world, parent);
            world.resource_mut::<State>().workspace.panel = panel;
            render(&mut world, parent);
            assert_eq!(world.resource::<Assets<Image>>().len(), 1);
            assert_eq!(world.resource::<State>().workspace.selected, trip);
            let labels: Vec<_> = world
                .query::<&Text>()
                .iter(&world)
                .map(|text| text.0.clone())
                .collect();
            assert!(!labels.iter().any(|label| label == "Another person's trip"));
        }
        crate::workflows::apply(&mut world, crate::workflows::Intent::New("Ride".into())).unwrap();
        let request = world
            .resource::<crate::workflows::State>()
            .form
            .as_ref()
            .unwrap()
            .request_id
            .clone();
        assert!(apply(&mut world, Intent::Back).is_err());
        assert!(active(&world));
        assert_eq!(
            world
                .resource::<crate::workflows::State>()
                .form
                .as_ref()
                .unwrap()
                .request_id,
            request
        );
        world.resource_mut::<Mobile>().identity =
            crate::session::Identity::Person(nucleus::new_uid("r"));
        assert!(!active(&world));
        crate::map::maintain(&mut world);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }
}
