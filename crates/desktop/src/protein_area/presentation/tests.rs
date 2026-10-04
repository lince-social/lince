use super::*;
use bevy::text::EditableText;
use serde_json::json;

pub(super) fn fixture() -> (App, Entity, Entity, Vec<Entity>) {
    let (mut app, root, owner) = super::super::tests::fixture();
    app.add_plugins(crate::edit_mode::EditModePlugin);
    app.update();
    let mut config = Config::default();
    config.bindings = ["head", "body", "quantity"]
        .into_iter()
        .map(|property| {
            let mut binding = Binding::new(property);
            binding.editable = true;
            binding
        })
        .collect();
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config.clone());
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        super::super::State {
            applied: Some(config),
            data: vec![
                json!({"uid":"first", "head":"First", "body":"Description", "quantity":"7"}),
                json!({"uid":"second", "head":"Second", "body":"Other", "quantity":"3"}),
            ],
            ready: true,
            dirty: true,
            ..default()
        },
    );
    super::super::rows::reconcile(app.world_mut(), owner);
    update(app.world_mut());
    let entities = ["first", "second"]
        .map(|uid| app.world().resource::<Runtime>().areas[&owner].row_entities[uid])
        .to_vec();
    (app, root, owner, entities)
}

fn editor(world: &mut World, row: Entity, property: &str) -> Entity {
    let mut todo = vec![row];
    while let Some(entity) = todo.pop() {
        if world.get::<EditableText>(entity).is_some() {
            let mut ancestor = entity;
            while ancestor != row {
                if world
                    .get::<super::super::rows::PropertyContainer>(ancestor)
                    .is_some_and(|container| container.0 == property)
                {
                    return entity;
                }
                ancestor = world.get::<ChildOf>(ancestor).unwrap().parent();
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            todo.extend(children.iter());
        }
    }
    panic!("missing editor")
}

fn container(world: &World, editor: Entity) -> Entity {
    let mut entity = editor;
    loop {
        if world
            .get::<super::super::rows::PropertyContainer>(entity)
            .is_some()
        {
            return entity;
        }
        entity = world.get::<ChildOf>(entity).unwrap().parent();
    }
}

#[test]
fn switching_hides_fields_without_replacing_editors_or_sending_record_changes() {
    let (mut app, _, owner, rows) = fixture();
    let body = editor(app.world_mut(), rows[0], "body");
    app.world_mut()
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text("Private unsaved draft");
    let before = app.world().resource::<Runtime>().areas[&owner].data.clone();
    assert!(set_view(
        app.world_mut(),
        rows[0],
        template("Title Sand", Layout::Sand, &["head"])
    ));
    assert_eq!(
        app.world()
            .get::<EditableText>(body)
            .unwrap()
            .value()
            .to_string(),
        "Private unsaved draft"
    );
    assert_eq!(editor(app.world_mut(), rows[0], "body"), body);
    assert!(app.world().get::<crate::castle::Castle>(rows[0]).is_none());
    assert_eq!(app.world().resource::<Runtime>().areas[&owner].data, before);
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .pending
            .is_empty()
    );
    app.world_mut()
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text("Later draft");
    Undo(rows[0]).apply(app.world_mut(), rows[0]);
    assert_eq!(editor(app.world_mut(), rows[0], "body"), body);
    assert_eq!(
        app.world()
            .get::<EditableText>(body)
            .unwrap()
            .value()
            .to_string(),
        "Later draft"
    );
    assert!(current(app.world(), rows[0]).is_none());
    assert!(app.world().get::<crate::castle::Castle>(rows[0]).is_some());
}

#[test]
fn preview_compares_fields_and_retains_local_fills_and_extras() {
    let (mut app, root, _, rows) = fixture();
    Open(rows[0]).apply(app.world_mut(), root);
    let mut session = app.world_mut().get_mut::<Session>(root).unwrap();
    session.target = template("Task Castle", Layout::Castle, &["head", "due_date"]);
    session.include_missing = true;
    session.keep_extra = true;
    session
        .target
        .fills
        .insert("due_date".into(), "2026-10-08".into());
    let result = result(&session);
    assert_eq!(
        compare(&session.observed.fields, &session.target.fields),
        lince_interface::presentation::Comparison {
            matching: vec!["head".into()],
            missing: vec!["due_date".into()],
            extra: vec!["body".into(), "quantity".into()]
        }
    );
    assert_eq!(result.fields, ["head", "due_date", "body", "quantity"]);
    assert!(result.valid());
    drop(session);
    ui::apply(app.world_mut(), root);
    assert!(app.world().get::<Session>(root).is_none());
    let displayed: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(displayed.iter().any(|text| text == "2026-10-08"));
    assert_eq!(
        current(app.world(), rows[0]).unwrap().fills["due_date"],
        "2026-10-08"
    );
}

#[test]
fn cancel_is_inert_and_stale_drafts_require_an_updated_preview() {
    let (mut app, root, _, rows) = fixture();
    let body = editor(app.world_mut(), rows[0], "body");
    Open(rows[0]).apply(app.world_mut(), root);
    ui::cancel(app.world_mut(), root);
    assert!(current(app.world(), rows[0]).is_none());
    Open(rows[0]).apply(app.world_mut(), root);
    app.world_mut().get_mut::<Session>(root).unwrap().target =
        template("Title Sand", Layout::Sand, &["head"]);
    app.world_mut()
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text("Typed while previewing");
    ui::apply(app.world_mut(), root);
    assert!(
        app.world()
            .get::<Session>(root)
            .unwrap()
            .message
            .contains("changed")
    );
    assert!(current(app.world(), rows[0]).is_none());
    assert!(
        app.world()
            .get::<Session>(root)
            .unwrap()
            .observed
            .drafts
            .iter()
            .any(|draft| draft.field == "body"
                && draft.dirty
                && draft.text == "Typed while previewing")
    );
    ui::apply(app.world_mut(), root);
    assert_eq!(current(app.world(), rows[0]).unwrap().name, "Title Sand");
    assert_eq!(editor(app.world_mut(), rows[0], "body"), body);
}

#[test]
fn protein_change_and_undo_restore_each_record_override_without_resetting_edits() {
    let (mut app, _, owner, rows) = fixture();
    let body = editor(app.world_mut(), rows[1], "body");
    set_view(
        app.world_mut(),
        rows[0],
        template("Text Sand", Layout::Sand, &["body"]),
    );
    set_view(
        app.world_mut(),
        owner,
        template("Description Castle", Layout::Castle, &["head", "body"]),
    );
    assert!(
        rows.iter()
            .all(|row| current(app.world(), *row).unwrap().name == "Description Castle")
    );
    app.world_mut()
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text("Updated after apply");
    Undo(owner).apply(app.world_mut(), owner);
    assert_eq!(current(app.world(), rows[0]).unwrap().name, "Text Sand");
    assert!(current(app.world(), rows[1]).is_none());
    assert_eq!(
        app.world()
            .get::<EditableText>(body)
            .unwrap()
            .value()
            .to_string(),
        "Updated after apply"
    );
    assert_eq!(editor(app.world_mut(), rows[1], "body"), body);
}

#[test]
fn presentation_and_field_settings_survive_serialization_and_recreated_rows() {
    use crate::sand_settings::{Value as Setting, set};
    let (mut app, _, owner, rows) = fixture();
    set_view(
        app.world_mut(),
        rows[0],
        template(
            "Description Castle",
            Layout::Castle,
            &["head", "body", "due_date"],
        ),
    );
    let body = editor(app.world_mut(), rows[0], "body");
    let container = container(app.world(), body);
    assert!(set(
        app.world_mut(),
        container,
        "width",
        Some(Setting::Number(240.0))
    ));
    assert!(set(
        app.world_mut(),
        container,
        "wrap",
        Some(Setting::Toggle(false))
    ));
    app.world_mut().flush();
    let saved = crate::record_presentation::capture(
        app.world(),
        owner,
        app.world().get::<InfluenceArea>(owner).unwrap().clone(),
    );
    let restored: InfluenceArea =
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(restored.validate());
    for row in rows {
        app.world_mut().despawn(row);
    }
    app.world_mut().entity_mut(owner).insert(restored);
    app.world_mut()
        .resource_mut::<Runtime>()
        .areas
        .get_mut(&owner)
        .unwrap()
        .dirty = true;
    super::super::rows::reconcile(app.world_mut(), owner);
    update(app.world_mut());
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities["first"];
    let body = editor(app.world_mut(), row, "body");
    let container = self::container(app.world(), body);
    assert_eq!(
        current(app.world(), row).unwrap().name,
        "Description Castle"
    );
    assert_eq!(app.world().get::<Node>(container).unwrap().width, px(240));
    assert_eq!(
        app.world().get::<TextLayout>(body).unwrap().linebreak,
        bevy::text::LineBreak::NoWrap
    );
}

#[test]
fn settings_reject_invalid_values_and_reset_nested_text_to_its_defaults() {
    use crate::sand_settings::{Value as Setting, set};
    let (mut app, root, _, _) = fixture();
    let castle = app
        .world_mut()
        .spawn((crate::castle::Castle, ChildOf(root)))
        .id();
    let text = crate::sand_text::spawn(
        app.world_mut(),
        castle,
        crate::sand_text::SavedText {
            area: crate::sand_text::SandText::new(true),
            text: "Local text".into(),
        },
    );
    assert!(!set(
        app.world_mut(),
        text,
        "wrap",
        Some(Setting::Number(1.0))
    ));
    assert!(!set(
        app.world_mut(),
        text,
        "overflow",
        Some(Setting::Choice("Invalid".into()))
    ));
    assert!(set(
        app.world_mut(),
        text,
        "wrap",
        Some(Setting::Toggle(false))
    ));
    assert!(set(
        app.world_mut(),
        text,
        "overflow",
        Some(Setting::Choice("Grow".into()))
    ));
    app.world_mut().flush();
    let texts = crate::sand_text::snapshot(app.world(), castle);
    let saved: crate::sand_text::SavedText =
        serde_json::from_slice(&serde_json::to_vec(&texts[0]).unwrap()).unwrap();
    assert!(saved.validate());
    let restored = crate::sand_text::spawn(app.world_mut(), castle, saved);
    assert_eq!(
        app.world()
            .get::<crate::sand_text::SandText>(restored)
            .unwrap()
            .overflow,
        crate::sand_text::TextOverflow::Grow
    );
    assert_eq!(
        app.world().get::<TextLayout>(restored).unwrap().linebreak,
        bevy::text::LineBreak::NoWrap
    );
    assert!(set(app.world_mut(), restored, "wrap", None));
    assert!(set(app.world_mut(), restored, "overflow", None));
    app.world_mut().flush();
    assert_eq!(
        app.world().get::<TextLayout>(restored).unwrap().linebreak,
        bevy::text::LineBreak::WordBoundary
    );
    assert_eq!(
        app.world()
            .get::<crate::sand_text::SandText>(restored)
            .unwrap()
            .overflow,
        crate::sand_text::TextOverflow::Scroll
    );
}

#[test]
fn standalone_protein_can_preview_cancel_present_and_undo_without_creating_records() {
    let (mut app, root, _, _) = fixture();
    let castle = crate::protein_castle::spawn(
        app.world_mut(),
        root,
        1,
        bevy::math::DVec2::ZERO,
        crate::protein_castle::ProteinDraft::default(),
    );
    let before = app
        .world_mut()
        .query::<&InfluenceArea>()
        .iter(app.world())
        .count();
    Open(castle).apply(app.world_mut(), root);
    assert!(app.world().get::<Session>(root).is_some());
    ui::cancel(app.world_mut(), root);
    assert_eq!(
        app.world_mut()
            .query::<&InfluenceArea>()
            .iter(app.world())
            .count(),
        before
    );
    Open(castle).apply(app.world_mut(), root);
    app.world_mut().get_mut::<Session>(root).unwrap().target =
        template("Title Sand", Layout::Sand, &["head"]);
    ui::apply(app.world_mut(), root);
    let area = app.world().get::<QueryEditor>(castle).unwrap().0;
    assert_eq!(
        base(app.world(), castle)
            .unwrap()
            .presentation
            .unwrap()
            .name,
        "Title Sand"
    );
    assert!(
        app.world().resource::<Runtime>().areas[&area]
            .pending
            .iter()
            .all(|message| !matches!(message, cell::ClientMessage::Act { .. }))
    );
    Undo(castle).apply(app.world_mut(), root);
    assert!(app.world().get::<QueryEditor>(castle).is_none());
    assert!(app.world().get_entity(castle).is_ok());
    assert!(app.world().get_entity(area).is_err());
    assert_eq!(
        app.world_mut()
            .query::<&InfluenceArea>()
            .iter(app.world())
            .count(),
        before
    );
}

#[test]
fn unchanged_presentations_reuse_the_same_nodes_and_reject_invalid_saved_settings() {
    let (mut app, _, _, rows) = fixture();
    set_view(
        app.world_mut(),
        rows[0],
        template(
            "Description Castle",
            Layout::Castle,
            &["head", "body", "due_date"],
        ),
    );
    let count = app.world().entities().len();
    for _ in 0..10 {
        update(app.world_mut());
    }
    assert_eq!(app.world().entities().len(), count);
    let mut state = app.world().get::<State>(rows[0]).unwrap().clone();
    state
        .view
        .as_mut()
        .unwrap()
        .fills
        .insert("due_date".into(), "x".repeat(4097));
    assert!(!state.valid());
    state.view.as_mut().unwrap().fills.clear();
    state.fields.insert(
        "head".into(),
        Values(BTreeMap::from([(
            "width".into(),
            lince_interface::settings::Value::Number(f32::INFINITY),
        )])),
    );
    assert!(!state.valid());
}

#[test]
fn configuring_a_sand_keeps_its_layout_and_settings_survive_presentation_undo() {
    use crate::sand_settings::{Value as Setting, set};
    let (mut app, _, _, rows) = fixture();
    let body = editor(app.world_mut(), rows[0], "body");
    assert!(set(
        app.world_mut(),
        rows[0],
        "spacing",
        Some(Setting::Number(14.0))
    ));
    app.world_mut().flush();
    assert!(current(app.world(), rows[0]).is_none());
    assert_eq!(app.world().get::<Node>(rows[0]).unwrap().row_gap, px(14));
    set_view(
        app.world_mut(),
        rows[0],
        template("Title Sand", Layout::Sand, &["head"]),
    );
    assert!(set(
        app.world_mut(),
        rows[0],
        "spacing",
        Some(Setting::Number(20.0))
    ));
    app.world_mut().flush();
    Undo(rows[0]).apply(app.world_mut(), rows[0]);
    assert!(current(app.world(), rows[0]).is_none());
    assert_eq!(app.world().get::<Node>(rows[0]).unwrap().row_gap, px(20));
    assert_eq!(editor(app.world_mut(), rows[0], "body"), body);
    assert!(set(app.world_mut(), rows[0], "spacing", None));
    app.world_mut().flush();
    assert_eq!(app.world().get::<Node>(rows[0]).unwrap().row_gap, px(8));
}

#[test]
fn clicking_inside_a_castle_selects_the_nearest_configurable_sand() {
    use bevy::{
        camera::NormalizedRenderTarget,
        ecs::system::RunSystemOnce,
        picking::{
            backend::HitData,
            hover::HoverMap,
            pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
        },
    };
    let (mut app, root, _, rows) = fixture();
    let body = editor(app.world_mut(), rows[0], "body");
    let container = container(app.world(), body);
    crate::edit_mode::show_customization(app.world_mut(), root);
    app.init_resource::<HoverMap>()
        .add_message::<PointerInput>();
    app.world_mut()
        .resource_mut::<HoverMap>()
        .entry(PointerId::Mouse)
        .or_default()
        .insert(body, HitData::new(root, 0.0, None, None));
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::None {
                width: 800,
                height: 600,
            },
            position: Vec2::ZERO,
        },
        PointerAction::Press(PointerButton::Primary),
    ));
    app.world_mut()
        .run_system_once(crate::sand_settings::select)
        .unwrap();
    assert_eq!(
        app.world().get::<crate::customization::Scope>(root),
        Some(&crate::customization::Scope::Sand(container))
    );
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "body Sand settings")
    );
}

#[test]
fn field_dimensions_and_wrapping_take_priority_over_automatic_editor_sizing() {
    use crate::sand_settings::{Value as Setting, set};
    let (mut app, _, _, rows) = fixture();
    let head = editor(app.world_mut(), rows[0], "head");
    let field = container(app.world(), head);
    app.world_mut()
        .entity_mut(head)
        .insert(bevy::text::TextLayoutInfo::default());
    for (id, value) in [
        ("width", Setting::Number(700.0)),
        ("height", Setting::Number(90.0)),
        ("wrap", Setting::Toggle(false)),
    ] {
        assert!(set(app.world_mut(), field, id, Some(value)));
    }
    app.world_mut().flush();
    for _ in 0..5 {
        super::super::rows::layout(app.world_mut());
    }
    let node = app.world().get::<Node>(field).unwrap();
    assert_eq!(node.width, px(700));
    assert_eq!(node.max_width, px(700));
    assert_eq!(node.height, px(90));
    assert_eq!(app.world().get::<Node>(head).unwrap().height, px(82));
    assert_eq!(
        app.world().get::<TextLayout>(head).unwrap().linebreak,
        bevy::text::LineBreak::NoWrap
    );
}
