use super::*;

fn panel_fixture(world: &mut World, center: DVec2) -> (Entity, Entity) {
    let root = world
        .spawn((
            crate::canvas::CanvasView { center, zoom: 2.0 },
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let sand = world.spawn((Node::default(), ChildOf(root))).id();
    let owner = populate(world, root, sand);
    world
        .get_mut::<WorkspaceSync>(owner)
        .unwrap()
        .replica_workspace = Some(1);
    (root, owner)
}

#[test]
fn shared_snapshots_preserve_personal_cameras_selection_and_policy_drafts() {
    let mut app = crate::sand_panel::tests::app();
    let world = app.world_mut();
    let (first_root, first) = panel_fixture(world, DVec2::new(10.0, 20.0));
    let (second_root, second) = panel_fixture(world, DVec2::new(90.0, 40.0));
    let element = Element {
        id: nucleus::new_uid("placement"),
        component: LayoutComponent::Builtin {
            state: ComponentState::Text {
                text: "Shared".into(),
            },
        },
        geometry: Geometry {
            position: [0.0; 2],
            size: [100.0; 2],
        },
    };
    let mut snapshot = json!({"name":"Joint","revision":1,"layout":{"elements":[element],"areas":{},"disabled_areas":[]},"separate_view":false,"can_edit":true,"can_review":true,"policy":{"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}});
    for owner in [first, second] {
        install(world, owner, &snapshot);
    }
    status(world, first, "Draft saved on this computer.");
    let replica = world.get::<WorkspaceSync>(first).unwrap().replicas[&element.id];
    world
        .entity_mut(first_root)
        .insert(crate::canvas_selection::SandSelection(vec![replica]));
    let policy_field = world.get::<WorkspaceSync>(first).unwrap().policy;
    *world.get_mut::<EditableText>(policy_field).unwrap() =
        crate::sand::editable("my unsaved policy draft");
    snapshot["revision"] = json!(2);
    snapshot["layout"]["elements"][0]["geometry"]["position"] = json!([25.0, 30.0]);
    for owner in [first, second] {
        install(world, owner, &snapshot);
    }
    assert_eq!(
        world
            .get::<Text>(world.get::<WorkspaceSync>(first).unwrap().status)
            .unwrap()
            .0,
        "Draft saved on this computer."
    );
    assert!(
        world
            .get::<Text>(world.get::<WorkspaceSync>(first).unwrap().workspace_status)
            .unwrap()
            .0
            .contains("revision 2")
    );
    assert_eq!(
        world.get::<WorkspaceSync>(first).unwrap().replicas[&element.id],
        replica
    );
    assert_eq!(
        world
            .get::<crate::canvas::CanvasItem>(replica)
            .unwrap()
            .position,
        DVec2::new(25.0, 30.0)
    );
    assert_eq!(
        world
            .get::<crate::canvas::CanvasView>(first_root)
            .unwrap()
            .center,
        DVec2::new(10.0, 20.0)
    );
    assert_eq!(
        world
            .get::<crate::canvas::CanvasView>(second_root)
            .unwrap()
            .center,
        DVec2::new(90.0, 40.0)
    );
    assert_eq!(
        world
            .get::<crate::canvas_selection::SandSelection>(first_root)
            .unwrap()
            .0,
        vec![replica]
    );
    assert_eq!(
        panel::value(world, policy_field).unwrap(),
        "my unsaved policy draft"
    );
}

#[test]
fn a_separate_permitted_view_does_not_accept_frames_from_the_shared_subscription() {
    let mut app = crate::sand_panel::tests::app();
    let world = app.world_mut();
    let (_, owner) = panel_fixture(world, DVec2::ZERO);
    let workspace = nucleus::new_uid("workspace");
    {
        let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
        view.selected = Some(workspace.clone());
        view.permitted = true;
        view.revision = 5;
    }
    receive(
        world,
        &Source::Local,
        &ServerMessage::Workspace {
            id: subscription(owner),
            workspace: json!({"hosted_workspace":workspace,"revision":6,"layout":{"elements":[],"areas":{}},"can_edit":true,"separate_view":false}),
        },
    );
    let view = world.get::<WorkspaceSync>(owner).unwrap();
    assert!(view.permitted);
    assert_eq!(view.revision, 5);
    assert!(!view.editable);
    let password = view.password;
    assert!(world.get::<crate::sand_text::SandText>(password).is_none());
}

#[test]
fn closing_a_workspace_panel_removes_its_personal_replica_and_feed_state() {
    let mut app = crate::sand_panel::tests::app();
    let world = app.world_mut();
    let (_, owner) = panel_fixture(world, DVec2::ZERO);
    install(
        world,
        owner,
        &json!({"revision":1,"layout":{"elements":[{"id":nucleus::new_uid("placement"),"component":LayoutComponent::Builtin { state:ComponentState::Text { text:"Replica".into() } },"geometry":{"position":[0.0,0.0],"size":[100.0,100.0]}}],"areas":{},"disabled_areas":[]},"separate_view":true}),
    );
    let replicas: Vec<_> = world
        .get::<WorkspaceSync>(owner)
        .unwrap()
        .replicas
        .values()
        .copied()
        .collect();
    assert_eq!(replicas.len(), 1);
    world.entity_mut(owner).despawn();
    maintain(world);
    assert!(!world.resource::<ReplicaFeeds>().0.contains_key(&owner));
    for replica in replicas {
        assert!(world.get_entity(replica).is_err());
    }
}

#[test]
fn acknowledging_a_canvas_change_keeps_the_loaded_drafts_original_host_and_revision() {
    let mut app = crate::sand_panel::tests::app();
    let world = app.world_mut();
    let (_, owner) = panel_fixture(world, DVec2::ZERO);
    let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
    view.draft_base = Some(1);
    view.draft_workspace = Some("original".into());
    view.draft_host = Some(Source::Organ("original-host".into()));
    view.pending = Some(("canvas-change".into(), Kind::Change));
    receive(
        world,
        &Source::Local,
        &ServerMessage::ActionOk {
            id: "canvas-change".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(json!({"state":"applied","revision":2})),
        },
    );
    let view = world.get::<WorkspaceSync>(owner).unwrap();
    assert_eq!(view.draft_base, Some(1));
    assert_eq!(view.draft_workspace.as_deref(), Some("original"));
    assert_eq!(view.draft_host, Some(Source::Organ("original-host".into())));
}
