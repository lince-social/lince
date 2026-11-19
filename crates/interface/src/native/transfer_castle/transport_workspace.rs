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

fn display(world: &mut World, entity: Entity, display: Display) {
    if let Some(mut node) = world.get_mut::<Node>(entity)
        && node.display != display
    {
        node.display = display;
    }
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
        world.entity_mut(owner).remove::<SupplierMatches>();
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.pending = None;
        view.preview = None;
        view.preview_request = None;
        view.disclosure_preview = None;
        view.notice = None;
        view.receipt = None;
        view.receipt_list = Value::Null;
        view.receipt_export = None;
        view.receipt_file_job = None;
    }
    if scoped
        && !world
            .get::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .matches_scope(&scope)
    {
        world
            .get_mut::<TransferCastle>(owner)
            .unwrap()
            .workflow
            .scope(&scope);
    }
    let active = world
        .get::<TransferCastle>(owner)
        .unwrap()
        .workflow
        .workflow
        == Workflow::Transport;
    let view = world.get::<View>(owner).unwrap();
    let (header, controls, summary, body, existing) = (
        view.header,
        view.controls,
        view.summary,
        world.get::<ChildOf>(view.list).unwrap().parent(),
        view.transport,
    );
    for entity in [header, controls, summary, body] {
        display(
            world,
            entity,
            if active { Display::None } else { Display::Flex },
        );
    }
    if !active {
        if let Some(entity) = existing {
            if previous == Workflow::Transport {
                world.despawn(entity);
                world.get_mut::<View>(owner).unwrap().transport = None;
            } else {
                display(world, entity, Display::None);
            }
        }
        return false;
    }
    let castle = world.get::<TransferCastle>(owner).unwrap();
    let workflow = castle.workflow.clone();
    let selected = castle.selected.clone();
    let has_form = castle.form.is_some();
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
        world.entity_mut(controls).insert((
            ZIndex(2),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ));
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
    display(
        world,
        root,
        if has_form {
            Display::None
        } else {
            Display::Flex
        },
    );
    let surface = world.get::<Surface>(root).unwrap();
    let (map, controls, panel) = (surface.map, surface.controls, surface.panel);
    ui::clear(world, panel);
    let (chosen, page, total, titles) = {
        let _span = nucleus::performance::Span::new("transfer-page-preparation");
        let rows = &world.get::<View>(owner).unwrap().rows;
        let explicit = rows.iter().find(|row| {
            model::display_ref(&row["uid"]) == selected && model::workspace::is_transport(row)
        });
        let visible = if workflow.panel == Panel::List || explicit.is_none() {
            workflow.rows(rows, &person)
        } else {
            Vec::new()
        };
        let chosen = explicit
            .or_else(|| {
                visible
                    .iter()
                    .copied()
                    .find(|row| model::display_ref(&row["uid"]) == workflow.selected)
                    .or_else(|| visible.first().copied())
            })
            .cloned();
        let page = workflow.page.min(visible.len().saturating_sub(1) / 30);
        let titles: Vec<_> = if workflow.panel == Panel::List {
            visible
                .iter()
                .skip(page * 30)
                .take(30)
                .map(|row| (title(row), text(row, "uid")))
                .collect()
        } else {
            Vec::new()
        };
        nucleus::performance::add("transfer-page-cards-prepared", titles.len() as u64);
        (chosen, page, visible.len(), titles)
    };
    if let Some(row) = &chosen {
        let uid = text(row, "uid");
        let mut castle = world.get_mut::<TransferCastle>(owner).unwrap();
        if castle.selected != uid {
            castle.selected.clone_from(&uid);
        }
        if castle.workflow.selected != uid {
            castle.workflow.selected = uid;
        }
    }
    crate::native::map_castle::transport_view(
        world,
        map,
        chosen.as_ref().map(Workspace::records).unwrap_or_default(),
        person.clone(),
        workflow.panel == Panel::Map,
    );
    {
        use crate::native::retained::Presentation;
        let _span = nucleus::performance::Span::new("transfer-list-presentation");
        if Presentation::matches(world, controls, |old: &Entity| *old == owner) {
            nucleus::performance::add("transfer-regions-retained", 1);
        } else {
            ui::clear(world, controls);
            button(world, controls, owner, "Back", Navigate::Back);
            button(
                world,
                controls,
                owner,
                "Transport options",
                Navigate::Panel(Panel::Options),
            );
            Presentation::remember(world, controls, owner);
            nucleus::performance::add("transfer-regions-rebuilt", 1);
        }
    }
    let expanded = matches!(
        workflow.panel,
        Panel::Details | Panel::List | Panel::Options
    );
    {
        let mut node = world.get_mut::<Node>(panel).unwrap();
        let height = if expanded { percent(88) } else { percent(35) };
        if node.max_height != height {
            node.max_height = height;
        }
        let display = if workflow.panel == Panel::Map {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
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
    match workflow.panel {
        Panel::List => {
            if workflow.page != page {
                world
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .workflow
                    .page = page;
            }
            for (title, uid) in titles {
                button(world, panel, owner, &title, Navigate::Select(uid));
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
            if (page + 1) * 30 < total {
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
                if workflow.history {
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
    use crate::native::laboratory::driver::{
        Limits, Runner, Selector, Step, UiAction, UiBackend, UiCondition,
    };
    fn click(app: &mut App, owner: Entity, label: &str, visible: &str) {
        let step = Step::action(
            label,
            Selector::new(label).with_role("Button"),
            UiAction::Click,
            UiCondition::Visible(Selector::new(visible).with_role("Button")),
        )
        .with_fallback();
        let report = Runner::new(Limits::default()).run(&mut UiBackend::new(app, owner), &step);
        assert!(
            report.is_success(),
            "{} {:?} {:?}: {}",
            report.step,
            report.selector,
            report.outcome,
            report.diagnostics
        );
    }
    let mut app = super::tests::app();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        .add_plugins((
            bevy::input::InputPlugin,
            crate::native::actions::ActionsPlugin,
            bevy::camera::visibility::VisibilityPlugin,
        ));
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
    ui::render(app.world_mut(), owner);
    app.update();
    click(&mut app, owner, "Transport", "Transport options");
    let surface = app.world().get::<View>(owner).unwrap().transport.unwrap();
    let map = app.world().get::<Surface>(surface).unwrap().map;
    app.world_mut()
        .get_mut::<crate::native::map_castle::MapSand>(map)
        .unwrap()
        .camera
        .zoom = 12.0;
    for (label, visible) in [
        ("My active transports", "Collapse"),
        ("Transport options", "New delivery"),
        ("Collapse", "My active transports"),
    ] {
        click(&mut app, owner, label, visible);
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
    click(&mut app, owner, "Transport options", "New ride");
    click(&mut app, owner, "New ride", "Cancel");
    let request = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .as_ref()
        .unwrap()
        .request_id
        .clone();
    let hidden_back = Step::action(
        "guard hidden Back while composing",
        Selector::new("Back").with_role("Button"),
        UiAction::Click,
        UiCondition::Visible(Selector::new("Transport").with_role("Button")),
    )
    .with_fallback();
    let report =
        Runner::new(Limits::default()).run(&mut UiBackend::new(&mut app, owner), &hidden_back);
    assert_eq!(report.outcome, crate::testing::Outcome::Rejected);
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
    click(&mut app, owner, "Cancel", "Transport options");
    assert_eq!(
        app.world().get::<Node>(surface).unwrap().display,
        Display::Flex
    );
    click(&mut app, owner, "Back", "My active transports");
    click(&mut app, owner, "Back", "Transport");
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
    click(&mut app, owner, "Transport", "Transport options");
    let previous = app.world().get::<View>(owner).unwrap().transport.unwrap();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.pending = Some(Pending {
            id: "old-organ-action".into(),
            form: Form::action("Old Organ draft", json!({"action":"create-transfer-draft"})),
        });
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
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    assert!(app.world().get_entity(previous).is_err());
    let selected = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .selected
        .clone();
    app.world_mut()
        .write_message(CellMessage::new(cell::ServerMessage::ActionOk {
            id: "old-organ-action".into(),
            created: Some(nucleus::new_uid("r")),
            facts: 0,
            warnings: vec!["Old Organ private result".into()],
            data: None,
        }));
    app.update();
    assert_eq!(
        app.world().get::<TransferCastle>(owner).unwrap().selected,
        selected
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Old Organ private result"))
    );
}

crate::native::laboratory_cases! {
    map_first_transport_panels_preserve_map_and_guard_an_open_draft,
}
