use super::*;
use crate::native::{actions::Action, area::InfluenceArea, protein_area::Source};

fn bound(world: &mut World, entity: Entity) -> String {
    let uid = nucleus::new_uid("r");
    world.entity_mut(entity).insert(RecordBinding {
        area: entity,
        uid: uid.clone(),
        source: Source::Local,
    });
    uid
}

#[test]
fn native_properties_reuse_the_record_factory_in_sandbox_and_park() {
    for park in [false, true] {
        for source in [Source::Local, Source::Organ("https://organ.example".into())] {
            let (mut world, root, native, _) = super::super::tests::fixture();
            if park {
                let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
                world.insert_resource(HostRequests(sender));
            }
            let uid = bound(&mut world, native);
            world.get_mut::<RecordBinding>(native).unwrap().source = source.clone();
            super::super::super::engine::resolve(&mut world);
            focus_item(&mut world, native);
            let position = world.get::<CanvasView>(root).unwrap().center;
            assert!(
                CommandContext::new(&world, root)
                    .unavailable(DesktopAction::InspectWindowRecord)
                    .is_none()
            );
            DesktopAction::InspectWindowRecord.apply(&mut world, root);
            let (area, config) = world
                .query::<(&InfluenceArea, &WorkspaceMember)>()
                .iter(&world)
                .find(|(area, _)| area.name == "Record Castle")
                .map(|(area, member)| (area.clone(), *member))
                .unwrap();
            assert_eq!(config.0, 1);
            assert_eq!(area.center, position.to_array());
            let config = area.protein.unwrap();
            assert_eq!(config.source, source);
            assert_eq!(config.draft.query["where"][0]["all"][0]["uid_eq"], uid);
        }
    }
}

#[test]
fn inspection_preserves_explicit_unfocus_and_rejects_unbound_or_invalid_native_records() {
    let (mut world, root, native, _) = super::super::tests::fixture();
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, native);
    assert!(capture(&world, root).is_err());
    bound(&mut world, native);
    assert!(capture(&world, root).is_ok());
    world.get_mut::<RecordBinding>(native).unwrap().uid = "a-slug".into();
    assert!(capture(&world, root).is_err());
    bound(&mut world, native);
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clear_focus();
    let before = *world.get::<CanvasView>(root).unwrap();
    DesktopAction::InspectWindowRecord.apply(&mut world, root);
    assert_eq!(world.query::<&InfluenceArea>().iter(&world).count(), 0);
    let after = world.get::<CanvasView>(root).unwrap();
    assert_eq!((after.center, after.zoom), (before.center, before.zoom));
    assert!(
        world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .focused()
            .is_none()
    );
}

#[test]
fn application_handoff_captures_the_origin_before_focus_or_workspace_changes() {
    let (mut world, root, native, application) = super::super::tests::fixture();
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    world.insert_resource(HostRequests(sender));
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, application);
    let position = world.get::<CanvasView>(root).unwrap().center.to_array();
    let placement = world.get::<Managed>(application).unwrap().id.0;
    DesktopAction::InspectWindowRecord.apply(&mut world, root);
    focus_item(&mut world, native);
    world.get_mut::<CanvasView>(root).unwrap().center += bevy::math::DVec2::splat(200.0);
    world.get_mut::<Workspaces>(root).unwrap().active = 44;
    assert_eq!(
        receiver.try_recv().unwrap(),
        crate::host::HostRequest::Inspect(WindowInspection {
            owner: root.to_bits(),
            workspace: 1,
            position,
            surface: SurfaceId(7),
            placement,
        })
    );
}

#[test]
fn a_transient_without_its_own_binding_cannot_inspect_its_parent() {
    let (mut world, root, native, application) = super::super::tests::fixture();
    bound(&mut world, native);
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    world.insert_resource(HostRequests(sender));
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    super::super::super::engine::resolve(&mut world);
    focus_item(&mut world, application);
    assert!(capture(&world, root).err().unwrap().contains("own Record"));
    DesktopAction::InspectWindowRecord.apply(&mut world, root);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn explicit_record_placement_rejects_a_missing_origin_without_opening_elsewhere() {
    let (mut world, root, _, _) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    let mut second = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 8;
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    world.get_mut::<Workspaces>(root).unwrap().active = 8;
    let position = bevy::math::DVec2::new(-900.0, 700.0);
    let area =
        crate::native::full_record::open_in(&mut world, root, 1, position, &uid, Source::Local)
            .unwrap();
    assert_eq!(world.get::<WorkspaceMember>(area).unwrap().0, 1);
    assert_eq!(
        world.get::<InfluenceArea>(area).unwrap().center,
        position.to_array()
    );
    for (workspace, position) in [
        (99, position),
        (1, bevy::math::DVec2::splat(f64::NAN)),
        (1, bevy::math::DVec2::splat(1e12)),
    ] {
        assert!(
            crate::native::full_record::open_in(
                &mut world,
                root,
                workspace,
                position,
                &uid,
                Source::Local
            )
            .is_none()
        );
    }
    assert_eq!(world.query::<&InfluenceArea>().iter(&world).count(), 1);
}

#[tokio::test]
async fn native_inspection_saves_owner_text_and_assertions_to_the_captured_record_after_close() {
    use crate::native::{app::CellHandle, cell_bridge, protein_area, wake::WakeSignal};
    use bevy::{input_focus::InputFocus, text::EditableText};
    use std::sync::Arc;

    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut uids = Vec::new();
    for head in ["Original owner", "Other owner"] {
        let result = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: head.into(),
                    body: format!("Description for {head}"),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
        uids.push(result.created.unwrap());
    }
    engine
        .act(
            engine::actions::Action::CreateConcept {
                lingua: "g_local".into(),
                name: "park-inspection-tag".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    let provider = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    };
    let (mut app, root, unused) = protein_area::tests::fixture();
    app.world_mut().despawn(unused);
    app.world_mut().entity_mut(root).insert((
        CanvasView::default(),
        ComputedNode {
            size: Vec2::new(1000.0, 700.0),
            inverse_scale_factor: 1.0,
            ..default()
        },
    ));
    let [native, other] = std::array::from_fn::<_, 2, _>(|_| {
        app.world_mut()
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::new(300.0, 200.0),
                },
                LayoutBox::new(Vec2::new(300.0, 200.0)),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id()
    });
    app.insert_resource(CellHandle(provider.clone()))
        .insert_resource(WakeSignal::new(|| {}))
        .add_plugins(cell_bridge::CellBridgePlugin);
    for (entity, uid) in [(native, &uids[0]), (other, &uids[1])] {
        app.world_mut()
            .entity_mut(entity)
            .remove::<ApplicationWindow>();
        app.world_mut().entity_mut(entity).insert(RecordBinding {
            area: entity,
            uid: uid.clone(),
            source: Source::Local,
        });
    }
    super::super::super::engine::resolve(app.world_mut());
    focus_item(app.world_mut(), native);
    DesktopAction::InspectWindowRecord.apply(app.world_mut(), root);
    let owner = app
        .world_mut()
        .query::<(Entity, &InfluenceArea)>()
        .iter(app.world())
        .find(|(_, area)| area.name == "Record Castle")
        .unwrap()
        .0;
    focus_item(app.world_mut(), other);
    let find = |world: &World| {
        let mut pending = vec![root];
        for _ in 0..8192 {
            let entity = pending.pop()?;
            if world
                .get::<EditableText>(entity)
                .is_some_and(|text| text.value().to_string() == "Description for Original owner")
                && world
                    .get::<RecordBinding>(entity)
                    .is_some_and(|binding| binding.area == owner && binding.uid == uids[0])
            {
                return Some(entity);
            }
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
        }
        None
    };
    protein_area::tests::until(&mut app, |world| find(world).is_some()).await;
    let editor = find(app.world()).unwrap();
    app.world_mut()
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Saved after original item closed");
    app.world_mut().despawn(native);
    let mut second = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 8;
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 8;
    protein_area::tests::until(&mut app, |world| {
        !protein_area::authoring_draft(world, editor)
    })
    .await;
    assert_eq!(
        app.world().get::<RecordBinding>(editor).unwrap().uid,
        uids[0]
    );
    assert_eq!(app.world().get::<WorkspaceMember>(owner).unwrap().0, 1);
    let form = app
        .world_mut()
        .query_filtered::<Entity, With<crate::native::assertion_editor::Field>>()
        .single(app.world())
        .unwrap();
    let input = crate::native::assertion_editor::input(app.world(), form).unwrap();
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("park-inspection-tag");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, bevy::input_focus::FocusCause::Pressed);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut verification = crate::native::full_record::config(&uids[0], Source::Local);
    verification.draft.query["where"] =
        serde_json::json!([{"any":[{"uid_eq":uids[0]},{"uid_eq":uids[1]}]}]);
    verification.draft.query["limit"] = serde_json::json!(2);
    let verification = verification.query().unwrap();
    loop {
        app.update();
        let replies = provider
            .local_session()
            .handle(cell::ClientMessage::Subscribe {
                id: "verify-inspection-save".into(),
                protein: verification.clone(),
            })
            .await;
        let rows = replies
            .iter()
            .find_map(|reply| match reply {
                cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            rows.len(),
            2,
            "Both inspected and unrelated Records must be verified"
        );
        let original = rows.iter().find(|row| row["uid"] == uids[0]).unwrap();
        let other = rows.iter().find(|row| row["uid"] == uids[1]).unwrap();
        assert_eq!(other["body"], "Description for Other owner");
        let tagged = original["assertions"].as_array().is_some_and(|assertions| {
            assertions
                .iter()
                .any(|assertion| assertion["predicate"] == "park-inspection-tag")
        });
        if tagged {
            assert_eq!(original["body"], "Saved after original item closed");
            assert!(!other["assertions"].as_array().is_some_and(|assertions| {
                assertions
                    .iter()
                    .any(|assertion| assertion["predicate"] == "park-inspection-tag")
            }));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "The captured Record assertion did not reach the actual Cell"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}
