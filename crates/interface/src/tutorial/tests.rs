use super::*;
use crate::{actions::ActionButton, icons::Tooltip};
use bevy::{a11y::AccessibilityNode, text::EditableText};

crate::laboratory_cases! { async tutorial_checks_real_protein_and_confirmed_entry_exit_changes, }

async fn until(app: &mut App, predicate: impl Fn(&mut World) -> bool) {
    for _ in 0..500 {
        app.update();
        if predicate(app.world_mut()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let errors: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .filter(|text| !text.is_empty())
        .collect();
    let areas: Vec<_> = app
        .world_mut()
        .query::<(
            &InfluenceArea,
            Option<&crate::area_mutation::MutationStatus>,
        )>()
        .iter(app.world())
        .map(|(area, status)| {
            (
                area.name.clone(),
                area.changes.clone(),
                area.rules.clone(),
                status.map(|status| status.0.clone()),
            )
        })
        .collect();
    panic!("Tutorial did not reach expected state: {areas:?}; {errors:?}");
}

fn click(world: &mut World, owner: Entity, title: &str) {
    let action = world
        .query::<(
            &ActionButton,
            Option<&Tooltip>,
            Option<&AccessibilityNode>,
            Option<&crate::icons::IconButton>,
        )>()
        .iter(world)
        .find(|(button, tip, node, icon)| {
            button.target == owner
                && (tip.is_some_and(|tip| tip.0 == title)
                    || node.is_some_and(|node| node.label() == Some(title))
                    || icon.is_some_and(|icon| icon.label == title))
        })
        .map(|(button, _, _, _)| button.actions.clone())
        .unwrap_or_else(|| panic!("Missing button: {title}"));
    action.run(world, owner);
    world.flush();
}

fn input(world: &mut World, title: &str, index: usize, value: &str) {
    let mut fields: Vec<_> = world.query_filtered::<(Entity, Option<&AccessibilityNode>, Option<&Tooltip>), With<EditableText>>().iter(world).filter(|(_, node, tip)| {
        node.is_some_and(|node| node.label() == Some(title)) || tip.is_some_and(|tip| tip.0 == title)
    }).map(|(entity, _, _)| entity).collect();
    fields.sort_by_key(|entity| {
        world
            .get::<ChildOf>(*entity)
            .and_then(|parent| world.get::<Children>(parent.parent()))
            .and_then(|children| children.iter().position(|child| child == *entity))
            .unwrap_or_default()
    });
    let entity = *fields
        .get(index)
        .unwrap_or_else(|| panic!("Missing field: {title}"));
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

fn choose(world: &mut World, owner: Entity, name: &str, choice: &str) {
    let toggle = world
        .query::<(Entity, &crate::dropdown::Dropdown, &AccessibilityNode)>()
        .iter(world)
        .find(|(_, _, node)| node.label() == Some(name))
        .unwrap()
        .0;
    world.trigger(bevy::ui_widgets::Activate { entity: toggle });
    world.flush();
    let menu = world.get::<crate::dropdown::Dropdown>(toggle).unwrap().menu;
    let action = world
        .query::<(&ActionButton, &AccessibilityNode, &ChildOf)>()
        .iter(world)
        .find(|(button, node, parent)| {
            button.target == owner && node.label() == Some(choice) && parent.parent() == menu
        })
        .map(|(button, _, _)| button.actions.clone())
        .unwrap();
    action.run(world, owner);
    world.flush();
}

fn selected(world: &World, root: Entity) -> Entity {
    world
        .get::<crate::area_panel::AreaEditor>(root)
        .unwrap()
        .selected
        .unwrap()
}

fn move_sample(world: &mut World, root: Entity, inside: bool) {
    let session = world.get::<Session>(root).unwrap();
    let uid = session.records[0].clone();
    let area = owned(world, root, session.change).unwrap();
    let mut point = DVec2::from_array(area.center);
    if !inside {
        point.x -= area.size[0] + 400.0;
    }
    let entity = rows(world, root)
        .iter()
        .find(|(_, record, _, _)| *record == uid)
        .unwrap()
        .0;
    crate::topology::set_position(world, entity, bevy::math::DVec3::new(point.x, 0.0, point.y));
}

#[cfg_attr(test, tokio::test)]
async fn tutorial_checks_real_protein_and_confirmed_entry_exit_changes() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Unrelated Record".into(),
                body: String::new(),
                quantity: 12.0,
            },
            None,
        )
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        crate::cell_bridge::CellBridgePlugin,
        crate::protein_area::ProteinAreaPlugin,
        crate::protein_castle::ProteinCastlePlugin,
        crate::area_mutation::AreaMutationPlugin,
        crate::edit_mode::EditModePlugin,
        crate::canvas_controls::CanvasControlsPlugin,
        TutorialPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::theme::Typography>()
    .init_resource::<bevy::input_focus::InputFocus>()
    .insert_resource(crate::app::CellHandle(runtime))
    .insert_resource(crate::wake::WakeSignal::new(|| {}))
    .add_systems(
        PostUpdate,
        crate::area_panel::autosave.before(crate::actions::ApplyActions),
    );
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    app.update();
    click(app.world_mut(), root, "Edit mode");
    click(app.world_mut(), root, "Areas of influence");
    click(app.world_mut(), root, "Add square");
    let existing = selected(app.world(), root);
    let before = app.world().get::<InfluenceArea>(existing).unwrap().clone();
    crate::edit_mode::EditAction::Credits.apply(app.world_mut(), root);
    crate::edit_mode::EditAction::Close.apply(app.world_mut(), root);
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .clear();
    Start.apply(app.world_mut(), root);
    let workspace = app.world().get::<Workspaces>(root).unwrap().active;
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries.len(),
        1
    );
    until(&mut app, |world| {
        world.get::<Session>(root).unwrap().records.len() == 2
    })
    .await;
    assert!(
        verify(app.world_mut(), root)
            .unwrap_err()
            .contains("Add square")
    );
    assert_eq!(*app.world().get::<InfluenceArea>(existing).unwrap(), before);
    let corner = current(app.world_mut(), root, "bottom-right corner");
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(corner, bevy::input_focus::FocusCause::Navigated);
    app.update();
    let edit = current(app.world_mut(), root, "Open Edit mode");
    assert_eq!(
        app.world()
            .get::<crate::edit_mode::EditControl>(edit)
            .unwrap()
            .action,
        crate::edit_mode::EditAction::Toggle
    );
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = workspace + 1;
    app.update();
    assert_eq!(
        app.world_mut()
            .query::<&TutorialHighlight>()
            .iter(app.world())
            .count(),
        0
    );
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = workspace;
    app.update();
    current(app.world_mut(), root, "Open Edit mode");
    click(app.world_mut(), root, "Edit mode");
    current(app.world_mut(), root, "Open Areas of influence");
    click(app.world_mut(), root, "Areas of influence");
    current(app.world_mut(), root, "Add a square");
    Command::Step(4).apply(app.world_mut(), root);
    Command::Next.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Session>(root).unwrap().step, 0);
    let prefix = app
        .world()
        .get::<Session>(root)
        .unwrap()
        .sample_prefix
        .clone();
    click(app.world_mut(), root, "Add square");
    let source = selected(app.world(), root);
    current(app.world_mut(), root, "In Protein");
    click(app.world_mut(), source, "Make this a Protein Area");
    assert!(
        verify(app.world_mut(), root)
            .unwrap_err()
            .contains("Quantity")
    );
    choose(app.world_mut(), source, "Add property", "Quantity");
    current(app.world_mut(), root, "Protein pencil");
    assert!(
        app.world()
            .get::<guide::Guide>(root)
            .unwrap()
            .instructions
            .iter()
            .any(|instruction| instruction.done && instruction.text.contains("add Quantity"))
    );
    click(
        app.world_mut(),
        source,
        "Edit the query in a Protein Castle; changes return to this Area",
    );
    let editor = app
        .world_mut()
        .query_filtered::<Entity, With<crate::protein_castle::ProteinCastle>>()
        .single(app.world())
        .unwrap();
    current(app.world_mut(), root, "add a condition");
    click(app.world_mut(), editor, "Add a condition");
    current(app.world_mut(), root, "Enter Area lesson");
    choose(app.world_mut(), editor, "Condition", "Quantity =");
    let condition = current(app.world_mut(), root, "Choose Text contains");
    assert!(
        app.world()
            .get::<crate::dropdown::Dropdown>(condition)
            .is_some()
    );
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: condition });
    app.world_mut().flush();
    let option = current(app.world_mut(), root, "Choose Text contains");
    assert_eq!(
        app.world()
            .get::<AccessibilityNode>(option)
            .unwrap()
            .label(),
        Some("Text contains")
    );
    assert_ne!(condition, option);
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: condition });
    app.world_mut().flush();
    choose(app.world_mut(), editor, "Condition", "Text contains");
    let field = current(app.world_mut(), root, "Enter Area lesson");
    assert!(
        matches!(app.world().get::<TutorialField>(field), Some(TutorialField::Query(owner, path)) if *owner == editor && path.ends_with("/text_contains"))
    );
    input(app.world_mut(), "Property value", 0, &prefix);
    app.update();
    current(app.world_mut(), root, "Click Run");
    assert!(verify(app.world_mut(), root).is_err());
    click(
        app.world_mut(),
        editor,
        "Run this query and keep results live",
    );
    until(&mut app, |world| verify(world, root).is_ok()).await;
    current(app.world_mut(), root, "Continue to the next lesson");
    assert_eq!(rows(app.world_mut(), root).len(), 2);
    let sample = rows(app.world_mut(), root)[0].0;
    app.world_mut()
        .get_mut::<WorkspaceMember>(sample)
        .unwrap()
        .0 = workspace + 1;
    assert!(verify(app.world_mut(), root).is_err());
    app.world_mut()
        .get_mut::<WorkspaceMember>(sample)
        .unwrap()
        .0 = workspace;
    Command::Next.apply(app.world_mut(), root);
    click(app.world_mut(), root, "Add circle");
    let force = selected(app.world(), root);
    click(app.world_mut(), root, "Add property");
    click(app.world_mut(), root, "Title");
    input(app.world_mut(), "Equals", 0, &format!("{prefix} 1"));
    input(app.world_mut(), "Position X", 0, "500");
    app.update();
    let slider = app
        .world_mut()
        .query::<(Entity, &crate::slider::SliderSand, &AccessibilityNode)>()
        .iter(app.world())
        .find(|(_, _, node)| node.label() == Some("Area force strength"))
        .unwrap()
        .0;
    app.world_mut().trigger(crate::slider::SliderChanged {
        entity: slider,
        value: 100.0,
    });
    app.world_mut().flush();
    click(app.world_mut(), root, "Attract");
    click(app.world_mut(), root, "Unlimited");
    click(app.world_mut(), root, "General");
    click(
        app.world_mut(),
        root,
        "Toggle physics in the open workspace. Turning it off keeps every Sand in place.",
    );
    click(app.world_mut(), root, "Areas of influence");
    assert!(verify(app.world_mut(), root).is_err());
    crate::area::forces(app.world_mut());
    assert!(verify(app.world_mut(), root).is_ok());
    Command::Next.apply(app.world_mut(), root);
    assert!(verify(app.world_mut(), root).is_err());
    click(app.world_mut(), root, "Repel");
    assert!(verify(app.world_mut(), root).is_err());
    crate::area::forces(app.world_mut());
    assert!(verify(app.world_mut(), root).is_ok());
    Command::Next.apply(app.world_mut(), root);
    click(app.world_mut(), root, "General");
    click(
        app.world_mut(),
        root,
        "Toggle physics in the open workspace. Turning it off keeps every Sand in place.",
    );
    click(app.world_mut(), root, "Areas of influence");
    click(app.world_mut(), root, "Add square");
    let change = selected(app.world(), root);
    input(app.world_mut(), "Position X", 0, "500");
    click(app.world_mut(), root, "Add property");
    click(app.world_mut(), root, "Title");
    input(app.world_mut(), "Equals", 0, &format!("{prefix} 1"));
    app.update();
    let entry = current(app.world_mut(), root, "On entry");
    assert_eq!(
        app.world().get::<TutorialField>(entry),
        Some(&TutorialField::Quantity(change, true))
    );
    input(app.world_mut(), "Quantity", 0, "1");
    app.update();
    let exit = current(app.world_mut(), root, "On exit");
    assert_eq!(
        app.world().get::<TutorialField>(exit),
        Some(&TutorialField::Quantity(change, false))
    );
    input(app.world_mut(), "Quantity", 0, "2");
    app.update();
    current(app.world_mut(), root, "On entry");
    input(app.world_mut(), "Quantity", 0, "1");
    input(app.world_mut(), "Quantity", 1, "0");
    app.update();
    move_sample(app.world_mut(), root, false);
    until(&mut app, |world| {
        crate::area_mutation::armed(world, change)
            && rows(world, root)
                .iter()
                .all(|(entity, _, _, _)| !crate::area_mutation::pending(world, *entity))
    })
    .await;
    assert!(verify(app.world_mut(), root).is_err());
    crate::edit_mode::EditAction::Close.apply(app.world_mut(), root);
    guide::update(app.world_mut(), root, false);
    let highlighted: Vec<_> = app
        .world_mut()
        .query::<&TutorialHighlight>()
        .iter(app.world())
        .map(|highlight| highlight.target)
        .collect();
    assert!(highlighted.contains(&change));
    assert!(
        rows(app.world_mut(), root)
            .iter()
            .any(|(entity, uid, _, _)| highlighted.contains(entity)
                && Some(uid) == app.world().get::<Session>(root).unwrap().records.first())
    );
    crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
    move_sample(app.world_mut(), root, true);
    app.update();
    move_sample(app.world_mut(), root, false);
    until(&mut app, |world| {
        !crate::area_mutation::armed(world, change)
    })
    .await;
    assert!(
        verify(app.world_mut(), root)
            .unwrap_err()
            .contains("off and on")
    );
    click(app.world_mut(), root, "Change properties");
    click(app.world_mut(), root, "Change properties");
    assert!(crate::area_mutation::armed(app.world(), change));
    move_sample(app.world_mut(), root, true);
    assert!(verify(app.world_mut(), root).is_err());
    until(&mut app, |world| verify(world, root).is_ok()).await;
    let records = app.world().get::<Session>(root).unwrap().records.clone();
    Command::Next.apply(app.world_mut(), root);
    assert!(verify(app.world_mut(), root).is_err());
    move_sample(app.world_mut(), root, false);
    until(&mut app, |world| verify(world, root).is_ok()).await;
    Command::Next.apply(app.world_mut(), root);
    assert!(app.world().get::<Session>(root).unwrap().completed);
    let force_before = app.world().get::<InfluenceArea>(force).unwrap().clone();
    let change_before = app.world().get::<InfluenceArea>(change).unwrap().clone();
    Command::Close.apply(app.world_mut(), root);
    assert_eq!(
        app.world_mut()
            .query::<&TutorialHighlight>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().active,
        workspace
    );
    assert!(app.world().get::<Session>(root).unwrap().hidden);
    assert_eq!(
        *app.world().get::<InfluenceArea>(force).unwrap(),
        force_before
    );
    assert_eq!(
        *app.world().get::<InfluenceArea>(change).unwrap(),
        change_before
    );
    Start.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Session>(root).unwrap().records, records);
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries.len(),
        1
    );
}

fn current(world: &mut World, root: Entity, expected: &str) -> Entity {
    let verified = verify(world, root).is_ok();
    guide::update(world, root, verified);
    let guide = world.get::<guide::Guide>(root).unwrap();
    let instruction = guide
        .instructions
        .iter()
        .find(|instruction| !instruction.done)
        .unwrap();
    assert!(
        instruction.text.contains(expected),
        "Expected {expected}, got {}",
        instruction.text
    );
    let targets: Vec<_> = world
        .query::<(&TutorialHighlight, &Pickable)>()
        .iter(world)
        .map(|(highlight, pickable)| {
            assert!(!pickable.is_hoverable && !pickable.should_block_lower);
            highlight.target
        })
        .collect();
    assert_eq!(
        targets.len(),
        1,
        "The current instruction {expected} must highlight one real control"
    );
    targets[0]
}
