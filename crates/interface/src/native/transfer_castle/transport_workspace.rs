use super::*;
use crate::transfers::workspace::{Panel, Workflow, Workspace};

#[derive(Component)]
struct Surface {
    map: Entity,
    controls: Entity,
    panel: Entity,
}

#[derive(Clone)]
enum Navigate {
    Back,
    Panel(Panel),
    Select(String),
    History,
    Page(isize),
    Stop,
}

impl Action for Navigate {
    fn apply(&self, world: &mut World, owner: Entity) {
        ui::capture(world, owner);
        if world.get::<TransferCastle>(owner).unwrap().form.is_some() {
            status(
                world,
                owner,
                "Your draft is kept. Save or cancel it before leaving.",
            );
            return;
        }
        match self {
            Self::Back => {
                if let Some(selection) = world
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .workflow
                    .back()
                {
                    world.get_mut::<TransferCastle>(owner).unwrap().selected = selection;
                }
            }
            Self::Panel(panel) => {
                world
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .workflow
                    .panel = *panel
            }
            Self::Select(uid) => {
                let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
                castle.workflow.select(uid);
                castle.selected = uid.clone();
            }
            Self::History => {
                let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
                castle.workflow.history = !castle.workflow.history;
                castle.workflow.panel = Panel::List;
                castle.workflow.page = 0;
            }
            Self::Page(delta) => {
                let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
                castle.workflow.page = castle.workflow.page.saturating_add_signed(*delta);
            }
            Self::Stop => {
                let person = person(world, owner);
                if let Some(row) = selected(world, owner) {
                    let record = crate::transfers::transport::sharing_record(&row, &person);
                    runtime::stop_location(world, owner, record, person);
                }
            }
        }
        ui::render(world, owner);
    }
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, action: Navigate) {
    crate::native::sand_panel::button(world, parent, owner, label, action);
}

pub(super) fn scope(world: &World, owner: Entity) -> String {
    format!(
        "{}:{}:{}",
        crate::native::practice_cells::source(world, owner).unwrap_or_default(),
        text(&world.get::<View>(owner).unwrap().context, "organ"),
        person(world, owner)
    )
}

pub(super) fn render(world: &mut World, owner: Entity) -> bool {
    let person = person(world, owner);
    let scope = scope(world, owner);
    let previous = world
        .get::<TransferCastle>(owner)
        .unwrap()
        .workflow
        .workflow;
    let scoped = world.get::<View>(owner).unwrap().context["viewer"].is_object();
    if scoped
        && previous == Workflow::Transport
        && !world
            .get::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .matches_scope(&scope)
    {
        world.get_mut::<TransferCastle>(owner).unwrap().form = None;
        runtime::cancel(world, owner, "preview");
    }
    if scoped {
        world
            .get_mut::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .scope(&scope);
    }
    let castle = world.get::<TransferCastle>(owner).unwrap().clone();
    let active = castle.workflow.workflow == Workflow::Transport;
    let view = world.get::<View>(owner).unwrap();
    let (header, controls, summary, body, existing) = (
        view.header,
        view.controls,
        view.summary,
        world.get::<ChildOf>(view.list).unwrap().parent(),
        view.transport,
    );
    for entity in [header, controls, summary, body] {
        world.get_mut::<Node>(entity).unwrap().display =
            if active { Display::None } else { Display::Flex };
    }
    if !active {
        if let Some(entity) = existing {
            if previous == Workflow::Transport {
                world.despawn(entity);
                world.get_mut::<View>(owner).unwrap().transport = None;
            } else {
                world.get_mut::<Node>(entity).unwrap().display = Display::None;
            }
        }
        return false;
    }
    let root = existing.unwrap_or_else(|| {
        let root = ui::stack(world, owner);
        world.get_mut::<Node>(root).unwrap().flex_grow = 1.0;
        world.get_mut::<Node>(root).unwrap().min_height = px(0);
        world.get_mut::<Node>(root).unwrap().overflow = Overflow::clip();
        let map = crate::native::map_castle::mount(
            world,
            root,
            Default::default(),
            Vec::new(),
            person.clone(),
        );
        world.entity_mut(map).insert(Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            flex_direction: FlexDirection::Column,
            ..default()
        });
        let controls = ui::row(world, root);
        world.entity_mut(controls).insert(ZIndex(2));
        let panel = ui::scroll(world, root);
        world.entity_mut(panel).insert((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(0),
                max_height: percent(45),
                padding: UiRect::all(px(14)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ZIndex(2),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ));
        world.entity_mut(root).insert(Surface {
            map,
            controls,
            panel,
        });
        world.get_mut::<View>(owner).unwrap().transport = Some(root);
        root
    });
    world.get_mut::<Node>(root).unwrap().display = if castle.form.is_some() {
        Display::None
    } else {
        Display::Flex
    };
    let surface = world.get::<Surface>(root).unwrap();
    let (map, controls, panel) = (surface.map, surface.controls, surface.panel);
    ui::clear(world, controls);
    ui::clear(world, panel);
    let rows = world.get::<View>(owner).unwrap().rows.clone();
    let visible = castle.workflow.rows(&rows, &person);
    let explicit = rows
        .iter()
        .find(|row| text(row, "uid") == castle.selected && model::workspace::is_transport(row));
    let chosen = explicit.cloned().or_else(|| {
        visible
            .iter()
            .find(|row| text(row, "uid") == castle.workflow.selected)
            .or_else(|| visible.first())
            .map(|row| (*row).clone())
    });
    if let Some(row) = &chosen {
        let uid = text(row, "uid");
        let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
        castle.selected = uid.clone();
        castle.workflow.selected = uid;
    }
    crate::native::map_castle::transport_view(
        world,
        map,
        chosen.as_ref().map(Workspace::records).unwrap_or_default(),
        person.clone(),
        castle.workflow.panel == Panel::Map,
    );
    button(world, controls, owner, "Back", Navigate::Back);
    button(
        world,
        controls,
        owner,
        "Transport options",
        Navigate::Panel(Panel::Options),
    );
    let expanded = matches!(
        castle.workflow.panel,
        Panel::Details | Panel::List | Panel::Options
    );
    {
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.max_height = if expanded { percent(88) } else { percent(35) };
        node.display = if castle.workflow.panel == Panel::Map {
            Display::None
        } else {
            Display::Flex
        };
    }
    if expanded {
        button(
            world,
            panel,
            owner,
            "Collapse",
            Navigate::Panel(Panel::Summary),
        );
    } else {
        button(
            world,
            panel,
            owner,
            "My active transports",
            Navigate::Panel(Panel::List),
        );
    }
    match castle.workflow.panel {
        Panel::List => {
            let page = castle
                .workflow
                .page
                .min(visible.len().saturating_sub(1) / 30);
            world
                .get_mut::<TransferCastle>(owner)
                .unwrap()
                .workflow
                .page = page;
            for row in visible.iter().skip(page * 30).take(30) {
                button(
                    world,
                    panel,
                    owner,
                    &title(row),
                    Navigate::Select(text(row, "uid")),
                );
            }
            if page > 0 {
                button(
                    world,
                    panel,
                    owner,
                    "Previous transports",
                    Navigate::Page(-1),
                );
            }
            if (page + 1) * 30 < visible.len() {
                button(world, panel, owner, "More transports", Navigate::Page(1));
            }
            ui::button(
                world,
                panel,
                owner,
                "New ride",
                ui::Command::New("Ride".into()),
            );
        }
        Panel::Options => {
            if world.get::<View>(owner).unwrap().context["viewer"]["local"] == true {
                let people = world
                    .get::<View>(owner)
                    .unwrap()
                    .records
                    .iter()
                    .filter(|row| row["kind"] == "person")
                    .map(|row| (title(row), ui::Command::Person(text(row, "uid"))))
                    .collect();
                ui::picker(world, panel, owner, "Acting Person", people);
            }
            ui::button(
                world,
                panel,
                owner,
                "New ride",
                ui::Command::New("Ride".into()),
            );
            ui::button(
                world,
                panel,
                owner,
                "New delivery",
                ui::Command::New("Delivery".into()),
            );
            button(
                world,
                panel,
                owner,
                if castle.workflow.history {
                    "Show my active transports"
                } else {
                    "Find transports and history"
                },
                Navigate::History,
            );
            button(
                world,
                panel,
                owner,
                "Map settings and navigation",
                Navigate::Panel(Panel::Map),
            );
            if let Some(row) = &chosen {
                button(
                    world,
                    panel,
                    owner,
                    "All transport actions",
                    Navigate::Panel(Panel::Details),
                );
                if capability(row, "edit_terms") {
                    ui::button(
                        world,
                        panel,
                        owner,
                        "Edit transport",
                        ui::Command::Edit(false),
                    );
                }
            }
        }
        Panel::Details => {
            if let Some(row) = &chosen {
                detail::render(world, owner, panel, row);
            }
        }
        Panel::Summary => {
            if let Some(row) = &chosen {
                crate::native::edit_mode::label(world, panel, &title(row), 20.0);
                crate::native::edit_mode::label(
                    world,
                    panel,
                    &model::human(&model::status(row)),
                    13.0,
                );
                if let Some(form) = crate::transfers::transport::journey_forms(row, &person)
                    .into_iter()
                    .next()
                {
                    ui::button(
                        world,
                        panel,
                        owner,
                        &form.title.clone(),
                        ui::Command::Open(form),
                    );
                } else {
                    button(
                        world,
                        panel,
                        owner,
                        "View transport",
                        Navigate::Panel(Panel::Details),
                    );
                }
                button(world, panel, owner, "Stop my sharing", Navigate::Stop);
            } else {
                crate::native::edit_mode::label(world, panel, "No active transports", 20.0);
                ui::button(
                    world,
                    panel,
                    owner,
                    "New ride",
                    ui::Command::New("Ride".into()),
                );
            }
        }
        Panel::Map => {}
    }
    crate::native::edit_mode::label(
        world,
        panel,
        "Natural Earth · © OpenStreetMap contributors · ESA WorldCover",
        10.0,
    );
    true
}

#[cfg_attr(test, test)]
fn map_first_transport_panels_preserve_map_and_guard_an_open_draft() {
    let mut app = super::tests::app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let person = nucleus::new_uid("r");
    let trip = nucleus::new_uid("r");
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.clone(),
            selected: "general".into(),
            ..Default::default()
        },
    );
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.ready = true;
        view.context = json!({"organ":nucleus::new_uid("r"),"viewer":{"local":true},"capabilities":{"create":true}});
        view.rows = vec![
            json!({"uid":trip,"head":"My trip","primary_status":"active",
            "parties":[{"actor":person}],"promises":[{"transport":{}}]}),
        ];
    }
    ui::Command::Transport.apply(app.world_mut(), owner);
    let surface = app.world().get::<View>(owner).unwrap().transport.unwrap();
    let map = app.world().get::<Surface>(surface).unwrap().map;
    app.world_mut()
        .get_mut::<crate::native::map_castle::MapSand>(map)
        .unwrap()
        .camera
        .zoom = 12.0;
    for panel in [Panel::List, Panel::Options, Panel::Summary] {
        Navigate::Panel(panel).apply(app.world_mut(), owner);
        assert_eq!(
            app.world().get::<View>(owner).unwrap().transport,
            Some(surface)
        );
        assert_eq!(app.world().get::<Surface>(surface).unwrap().map, map);
        assert_eq!(
            app.world()
                .get::<crate::native::map_castle::MapSand>(map)
                .unwrap()
                .camera
                .zoom,
            12.0
        );
    }
    ui::Command::New("Ride".into()).apply(app.world_mut(), owner);
    let request = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .as_ref()
        .unwrap()
        .request_id
        .clone();
    Navigate::Back.apply(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<TransferCastle>(owner)
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .request_id,
        request
    );
    assert_eq!(
        app.world().get::<Node>(surface).unwrap().display,
        Display::None
    );
    ui::Command::Cancel.apply(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<Node>(surface).unwrap().display,
        Display::Flex
    );
    Navigate::Back.apply(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .workflow,
        Workflow::General
    );
    assert_eq!(
        app.world().get::<TransferCastle>(owner).unwrap().selected,
        "general"
    );
    assert!(app.world().resource::<Requests>().0.is_empty());
    ui::Command::Transport.apply(app.world_mut(), owner);
    let previous = app.world().get::<View>(owner).unwrap().transport.unwrap();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.context["organ"] = json!(nucleus::new_uid("r"));
        view.rows.clear();
    }
    ui::render(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .workflow,
        Workflow::General
    );
    assert!(app.world().get::<View>(owner).unwrap().transport.is_none());
    assert!(app.world().get_entity(previous).is_err());
}

crate::native::laboratory_cases! {
    map_first_transport_panels_preserve_map_and_guard_an_open_draft,
}
