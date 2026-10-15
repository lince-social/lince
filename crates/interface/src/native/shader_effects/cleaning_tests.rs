use super::*;
use crate::native::{canvas_host::Identity, topology::rooms, workspace::WorkspaceMember};
use std::sync::Arc;

#[test]
fn cleaning_preset_converts_an_existing_floor_without_losing_its_record_source_or_mapping() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    let root = world
        .spawn((
            Node::default(),
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let uid = nucleus::new_uid("r");
    let source = Source::Organ(nucleus::new_uid("r"));
    let mut settings = Surface {
        record: Some(uid.clone()),
        source: source.clone(),
        ..default()
    };
    settings.effect.inputs.task = Some(model::QuantityRange {
        start: "-3".into(),
        end: "0".into(),
    });
    settings.effect.inputs.progress = settings.effect.inputs.task.clone();
    let inputs = settings.effect.inputs.clone();
    let floor = world
        .spawn((
            settings.clone(),
            rooms::Member(nucleus::canvas::room::Member {
                room: nucleus::new_uid("placement"),
                parent: None,
                local: nucleus::canvas::room::identity_pose(),
                role: nucleus::canvas::room::Role::Floor,
            }),
        ))
        .id();
    let owner = open(
        &mut world,
        root,
        Target::Surface(floor),
        settings.effect,
        0,
        Some("-3".into()),
    );
    Command::Preset(model::CLEANING.into()).apply(&mut world, owner);
    let settings = world.get::<Surface>(floor).unwrap();
    assert_eq!(settings.record.as_deref(), Some(uid.as_str()));
    assert_eq!(settings.source, source);
    assert!(settings.room_ambient && settings.quantity_only && !settings.motion);
    assert_eq!(settings.glow, 0.0);
    let definition = &world.get::<Editor>(owner).unwrap().effect;
    assert_eq!(definition.inputs, inputs);
    assert_eq!(definition.programs[&definition.fallback], model::CLEANING);
}

#[test]
fn cleaning_reset_preserves_explicit_sources_and_mappings_and_resumes_freeze() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    let root = world
        .spawn((
            Node::default(),
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let mut settings = Surface::cleaning();
    let uid = nucleus::new_uid("r");
    settings.record = Some(uid.clone());
    settings.frozen = true;
    settings.effect.inputs.task = Some(model::QuantityRange {
        start: "-3".into(),
        end: "0".into(),
    });
    settings.effect.inputs.progress = settings.effect.inputs.task.clone();
    let inputs = settings.effect.inputs.clone();
    let floor = world.spawn(settings.clone()).id();
    let owner = open(
        &mut world,
        root,
        Target::Surface(floor),
        settings.effect,
        0,
        Some("-3".into()),
    );
    world
        .get_mut::<Editor>(owner)
        .unwrap()
        .effect
        .programs
        .insert("surface".into(), "fn shade(".into());
    Command::Reset.apply(&mut world, owner);
    let settings = world.get::<Surface>(floor).unwrap();
    assert_eq!(settings.record.as_deref(), Some(uid.as_str()));
    assert!(
        settings.quantity_only && settings.room_ambient && !settings.frozen && !settings.motion
    );
    assert_eq!(settings.glow, 0.0);
    let editor = world.get::<Editor>(owner).unwrap();
    assert_eq!(editor.effect.inputs, inputs);
    assert_eq!(
        editor.effect.programs[&editor.effect.fallback],
        model::CLEANING
    );
    assert!(
        !world
            .get::<shader::EffectPreview>(editor.preview)
            .unwrap()
            .frozen
    );
    Command::Disable.apply(&mut world, owner);
    assert!(world.get::<Surface>(floor).unwrap().disabled);
    Command::Disable.apply(&mut world, owner);
    assert!(!world.get::<Surface>(floor).unwrap().disabled);
    world
        .get_mut::<Editor>(owner)
        .unwrap()
        .effect
        .inputs
        .task
        .as_mut()
        .unwrap()
        .end = "invalid".into();
    Command::Reset.apply(&mut world, owner);
    assert_eq!(world.get::<Editor>(owner).unwrap().effect.inputs, inputs);
    assert!(
        world
            .get::<Editor>(owner)
            .unwrap()
            .effect
            .validate()
            .is_ok()
    );
}

async fn until(app: &mut App, condition: impl Fn(&World) -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            shader::prepare_test_previews(app.world_mut());
            app.update();
            if condition(app.world()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cleaning_record_backend_updates_restore_and_removal_use_the_real_feed() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::ensure_local(&engine.store.pool, "http://cleaning-room")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "key"))
        .await
        .unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Clean room".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<crate::native::tokens::ThemeSettings>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .init_resource::<bevy::picking::hover::HoverMap>()
        .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
            commands: default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            speech: None,
            information: None,
        }))
        .add_plugins((
            crate::native::cell_bridge::CellBridgePlugin,
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::record_binding::RecordBindingPlugin,
            crate::native::description::DescriptionPlugin,
            ShaderEffectsPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            Node::default(),
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let room_id = nucleus::new_uid("placement");
    app.world_mut().spawn((
        rooms::Room(default()),
        Identity(room_id.clone()),
        WorkspaceMember(1),
        ChildOf(root),
    ));
    let mut surface = Surface::cleaning();
    surface.record = Some(record.clone());
    surface.effect.inputs.task = Some(model::QuantityRange {
        start: "0".into(),
        end: "10".into(),
    });
    surface.effect.inputs.progress = surface.effect.inputs.task.clone();
    let member = rooms::Member(nucleus::canvas::room::Member {
        room: room_id,
        parent: None,
        local: nucleus::canvas::room::identity_pose(),
        role: nucleus::canvas::room::Role::Floor,
    });
    let mut floor = app
        .world_mut()
        .spawn((
            Node::default(),
            surface,
            member.clone(),
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    for (quantity, task, progress) in [
        (0.0, model::TaskState::Pending, 0.0),
        (5.0, model::TaskState::Active, 0.5),
        (10.0, model::TaskState::Completed, 1.0),
    ] {
        engine
            .act(
                engine::actions::Action::SetQuantity {
                    target: record.clone(),
                    value: quantity,
                },
                None,
            )
            .await
            .unwrap();
        until(&mut app, |world| {
            world
                .get::<Background>(floor)
                .and_then(|background| world.get::<shader::EffectPreview>(background.preview))
                .is_some_and(|preview| {
                    let inputs = preview.effect.inputs.resolve(preview.quantity.as_deref());
                    !preview.disabled && inputs.task == task && inputs.progress == Some(progress)
                })
        })
        .await;
        if quantity == 5.0 {
            let saved: Surface = serde_json::from_slice(
                &serde_json::to_vec(app.world().get::<Surface>(floor).unwrap()).unwrap(),
            )
            .unwrap();
            app.world_mut().despawn(floor);
            floor = app
                .world_mut()
                .spawn((
                    Node::default(),
                    saved,
                    member.clone(),
                    WorkspaceMember(1),
                    ChildOf(root),
                ))
                .id();
        }
    }
    engine
        .act(
            engine::actions::Action::DeleteRecord { target: record },
            None,
        )
        .await
        .unwrap();
    until(&mut app, |world| {
        world
            .get::<Background>(floor)
            .and_then(|background| world.get::<shader::EffectPreview>(background.preview))
            .is_some_and(|preview| preview.disabled && preview.quantity.is_none())
    })
    .await;
    app.world_mut().despawn(floor);
    until(&mut app, |world| world.resource::<Feeds>().0.is_empty()).await;
    assert_eq!(
        app.world_mut()
            .query::<&Background>()
            .iter(app.world())
            .count(),
        0
    );
}
