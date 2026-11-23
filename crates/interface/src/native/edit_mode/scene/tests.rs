use super::*;
use crate::native::{
    actions::ActionButton,
    edit_mode::{EditAction, EditControl},
};
use bevy::{a11y::AccessibilityNode, text::EditableText, ui_widgets::Activate};
use std::sync::Arc;

fn click(app: &mut App, entity: Entity) {
    app.world_mut().trigger(Activate { entity });
    app.update();
}

fn activate(app: &mut App, label: &str) {
    let w = app.world_mut();
    let entity = w
        .query_filtered::<(Entity, &AccessibilityNode), With<ActionButton>>()
        .iter(w)
        .find(|(_, n)| n.label() == Some(label))
        .unwrap_or_else(|| panic!("Missing control {label}"))
        .0;
    click(app, entity);
}

fn text(app: &mut App, label: &str, text: &str) {
    let w = app.world_mut();
    let mut fields = w.query::<(&AccessibilityNode, &mut EditableText)>();
    let (_, mut field) = fields
        .iter_mut(w)
        .find(|(n, _)| n.label() == Some(label))
        .unwrap();
    field.editor.set_text(text);
}

fn session(app: &App, root: Entity) -> &Session {
    &app.world().get::<Documents>(root).unwrap().0[&1]
}

fn open_inspector(app: &mut App, root: Entity) {
    for action in [EditAction::Toggle, EditAction::Workspaces] {
        let w = app.world_mut();
        let e = w
            .query::<(Entity, &EditControl)>()
            .iter(w)
            .find(|(_, c)| c.action == action && c.root == root)
            .unwrap()
            .0;
        click(app, e);
    }
    activate(app, "Scene document");
}

#[test]
fn native_conflict_retains_the_draft_and_can_save_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, root) = super::super::tests::fixture();
    open_inspector(&mut app, root);
    let original = dir.path().join("original.workspace");
    text(&mut app, "Scene directory", original.to_str().unwrap());
    activate(&mut app, "New scene");
    text(&mut app, "New part name", "Screw");
    activate(&mut app, "Create part and place it");
    activate(&mut app, "Place another copy of this part");
    activate(&mut app, "Save scene");
    let published = workspace_scene::open(&original).unwrap();
    let chosen = session(&app, root).selected.clone().unwrap();
    let first = published.scene.handle(&published.scene.slots[0]);
    assert_ne!(first, chosen);
    text(&mut app, "X (m)", "0.125");
    activate(&mut app, "Apply to selected placement");
    let editor = session(&app, root);
    let before = editor.undo.last().unwrap();
    let after = editor.scene.as_ref().unwrap();
    assert!(Arc::ptr_eq(&before.parts, &after.parts));
    assert!(!Arc::ptr_eq(&before.slots, &after.slots));
    eprintln!(
        "move retained {} definition bytes in one shared allocation and copied {} placement rows ({} inline bytes plus strings)",
        nucleus::karma::canonical_json_bytes(&*after.parts)
            .unwrap()
            .len(),
        after.slots.len(),
        after.slots.len() * std::mem::size_of::<nucleus::canvas::scene::Slot>()
    );
    activate(&mut app, "Undo scene edit");
    assert_eq!(
        session(&app, root).scene.as_ref().unwrap(),
        &published.scene
    );
    activate(&mut app, "Redo scene edit");
    activate(&mut app, "Close scene inspector");
    activate(&mut app, "Scene document");
    let draft = session(&app, root).scene.clone().unwrap();
    assert_eq!(draft.slot(&first), published.scene.slot(&first));
    assert_eq!(draft.slot(&chosen).unwrap().metres[0].to_string(), "0.125");
    let mut external = published.scene;
    external.add_part("Bracket").unwrap();
    workspace_scene::save(&original, &external, Some(&published.revision)).unwrap();
    activate(&mut app, "Save scene");
    assert!(session(&app, root).message.contains("revision conflict"));
    assert_eq!(session(&app, root).scene.as_ref().unwrap(), &draft);
    let copy = dir.path().join("draft.workspace");
    text(&mut app, "Scene directory", copy.to_str().unwrap());
    activate(&mut app, "Save to a new directory");
    assert_eq!(workspace_scene::open(&copy).unwrap().scene, draft);
    assert_eq!(workspace_scene::open(&original).unwrap().scene, external);
}

#[test]
fn library_rechecks_a_review_before_import_and_undo_preserves_both_existing_placements() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.workspace");
    crate::native::laboratory::showoff::parts_library(&source).unwrap();
    let published = workspace_scene::open(&source).unwrap();
    let mut library = published.scene;
    let bracket = library.slots[0].pin(&library.document);
    let steel = library.definition(&bracket).unwrap().requires[0].clone();
    let review = format!("Review Bracket · {}", &bracket.revision.as_str()[7..19]);
    let (mut app, root) = super::super::tests::fixture();
    open_inspector(&mut app, root);
    activate(&mut app, "New scene");
    text(&mut app, "New part name", "Bracket");
    activate(&mut app, "Create part and place it");
    activate(&mut app, "Place another copy of this part");
    let before = session(&app, root).scene.clone().unwrap();
    let depth = session(&app, root).undo.len();
    activate(&mut app, "Definition library");
    text(&mut app, "Source scene directory", source.to_str().unwrap());
    activate(&mut app, "Browse source definitions");
    activate(&mut app, &review);
    activate(&mut app, "Show source and revision details");
    activate(&mut app, "Hide source and revision details");
    library.set_local_only(&steel, true).unwrap();
    let restricted = workspace_scene::save(&source, &library, Some(&published.revision)).unwrap();
    activate(&mut app, "Import reviewed definitions");
    assert!(session(&app, root).message.contains("permissions changed"));
    assert_eq!(session(&app, root).scene.as_ref(), Some(&before));
    assert_eq!(session(&app, root).undo.len(), depth);
    activate(&mut app, "Browse source definitions");
    activate(&mut app, &review);
    assert!(session(&app, root).message.contains("Incomplete import"));
    let world = app.world_mut();
    assert!(
        !world
            .query_filtered::<&AccessibilityNode, With<ActionButton>>()
            .iter(world)
            .any(|n| n.label() == Some("Import reviewed definitions"))
    );
    library.set_local_only(&steel, false).unwrap();
    workspace_scene::save(&source, &library, Some(&restricted)).unwrap();
    activate(&mut app, "Browse source definitions");
    activate(&mut app, &review);
    activate(&mut app, "Import reviewed definitions");
    let imported = session(&app, root).scene.clone().unwrap();
    assert_eq!(imported.parts.len(), before.parts.len() + 3);
    assert!(Arc::ptr_eq(&imported.slots, &before.slots));
    activate(&mut app, "Undo scene edit");
    assert_eq!(session(&app, root).scene.as_ref(), Some(&before));
    activate(&mut app, "Redo scene edit");
    assert_eq!(session(&app, root).scene.as_ref(), Some(&imported));
}

#[test]
fn linked_room_uses_local_axes_and_rejects_cancelled_or_superseded_drags() {
    use crate::native::{
        canvas::CanvasItem,
        canvas_host::Identity,
        canvas_selection,
        item_properties::Attachment,
        topology::{
            self,
            rooms::{self, Member, Room},
        },
        workspace::WorkspaceMember,
    };
    use bevy::math::{DQuat, DVec2, DVec3};
    use nucleus::canvas::room;
    let (mut app, root) = super::super::tests::fixture();
    open_inspector(&mut app, root);
    activate(&mut app, "New scene");
    text(&mut app, "New part name", "Bracket");
    activate(&mut app, "Create part and place it");
    let room_id = nucleus::new_uid("placement");
    let record = nucleus::new_uid("r");
    let item_id = nucleus::new_uid("placement");
    let geometry = CanvasItem {
        position: DVec2::ZERO,
        size: Vec2::splat(100.),
    };
    let frame = app
        .world_mut()
        .spawn((
            Room(room::Room::default()),
            Identity(room_id.clone()),
            geometry,
            WorkspaceMember(1),
            ChildOf(root),
            topology::Spatial {
                rotation: DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2).to_array(),
                ..default()
            },
        ))
        .id();
    let member = room::Member {
        room: room_id,
        parent: Some(item_id.clone()),
        local: room::identity_pose(),
        role: room::Role::Content,
    };
    let item = app
        .world_mut()
        .spawn((
            Member(member),
            Identity(item_id.clone()),
            Attachment {
                uid: record.clone(),
            },
            geometry,
            WorkspaceMember(1),
            ChildOf(root),
        ))
        .id();
    canvas_selection::set_selection(app.world_mut(), root, vec![item]);
    activate(&mut app, "Link selected Room member to this placement");
    assert!(session(&app, root).scene.as_ref().unwrap().canvas.is_none());
    app.world_mut().get_mut::<Member>(item).unwrap().0.parent = None;
    activate(&mut app, "Link selected Room member to this placement");
    let before = session(&app, root).scene.clone().unwrap();
    assert_eq!(before.slots[0].record.as_deref(), Some(record.as_str()));
    let history = session(&app, root).undo.len();
    assert!(bridge::begin(app.world_mut(), root, item));
    topology::set_position(app.world_mut(), item, DVec3::new(50., 0., 0.));
    bridge::finish(app.world_mut(), root, false);
    assert_eq!(topology::position(app.world(), item), Some(DVec3::ZERO));
    assert_eq!(session(&app, root).undo.len(), history);
    assert!(bridge::begin(app.world_mut(), root, item));
    text(&mut app, "X (m)", "0.125");
    activate(&mut app, "Apply to selected placement");
    topology::set_position(app.world_mut(), item, DVec3::new(64., 0., 0.));
    bridge::finish(app.world_mut(), root, true);
    assert_eq!(
        session(&app, root).scene.as_ref().unwrap().slots[0].metres[0].to_string(),
        "0.125"
    );
    let expected = room::compose(
        &rooms::world_pose(app.world(), frame).unwrap(),
        &app.world().get::<Member>(item).unwrap().0.local,
    );
    assert!((DVec3::from_array(expected.position) - DVec3::new(0., 0., -12.5)).length() < 1e-10);
    assert_eq!(
        topology::position(app.world(), item),
        Some(DVec3::from_array(expected.position))
    );
    let duplicate = app
        .world_mut()
        .spawn((Identity(item_id), WorkspaceMember(1), ChildOf(root)))
        .id();
    activate(&mut app, "Undo scene edit");
    assert!(session(&app, root).message.contains("ambiguous"));
    assert_eq!(session(&app, root).undo.len(), history + 1);
    app.world_mut().despawn(duplicate);
    activate(&mut app, "Undo scene edit");
    assert_eq!(session(&app, root).scene.as_ref().unwrap(), &before);
}

#[test]
fn accepted_commands_preserve_native_drafts_and_recheck_revocation_at_publication() {
    use nucleus::{
        canvas::{
            Response,
            scene::operation::{Edit, Operation, Request},
        },
        component::composition::Origin,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scene.workspace");
    let (mut app, root) = super::super::tests::fixture();
    open_inspector(&mut app, root);
    text(&mut app, "Scene directory", path.to_str().unwrap());
    activate(&mut app, "New scene");
    text(&mut app, "New part name", "Bracket");
    activate(&mut app, "Create part and place it");
    activate(&mut app, "Save scene");
    let before = workspace_scene::open(&path).unwrap();
    let handle = before.scene.handle(&before.scene.slots[0]);
    let edit = Edit {
        request_id: nucleus::new_uid("request"),
        document: before.scene.document.clone(),
        expected: before.revision.clone(),
        operation: Operation::Move {
            target: handle.clone(),
            metres: [DecimalValue::from_integer(3); 3],
        },
    };
    let request = Request::Submit { edit: edit.clone() };
    let fiote = Origin {
        agent: nucleus::new_uid("r"),
        thread: nucleus::new_uid("r"),
    };
    let checks = std::cell::Cell::new(0);
    let revoked = accepted::request(app.world_mut(), root, 1, &request, Some(&fiote), || {
        let check = checks.get();
        checks.set(check + 1);
        check != 0
    });
    assert!(revoked.unwrap_err().contains("cancelled"));
    assert_eq!(
        workspace_scene::open(&path).unwrap().revision,
        before.revision
    );
    assert!(
        workspace_scene::acceptance::receipt(&path, &edit.document, Some(&edit.request_id))
            .is_err()
    );
    let response =
        accepted::request(app.world_mut(), root, 1, &request, Some(&fiote), || false).unwrap();
    let Response::SceneReceipt { receipt } = response else {
        panic!("Expected the Fiote receipt")
    };
    assert_eq!(receipt.fiote.as_ref(), Some(&fiote));
    text(&mut app, "X (m)", "7");
    activate(&mut app, "Apply to selected placement");
    let draft = session(&app, root).scene.clone().unwrap();
    assert!(session(&app, root).dirty);
    let recovered =
        accepted::request(app.world_mut(), root, 1, &request, Some(&fiote), || false).unwrap();
    assert_eq!(
        recovered,
        Response::SceneReceipt {
            receipt: receipt.clone()
        }
    );
    let another = Origin {
        agent: nucleus::new_uid("r"),
        thread: fiote.thread.clone(),
    };
    let lookup = Request::Receipt {
        document: edit.document.clone(),
        request_id: edit.request_id.clone(),
    };
    assert!(
        accepted::request(app.world_mut(), root, 1, &lookup, Some(&another), || false).is_err()
    );
    let fresh = Request::Submit {
        edit: Edit {
            expected: receipt.revision.clone(),
            request_id: nucleus::new_uid("request"),
            ..edit
        },
    };
    assert!(
        accepted::request(app.world_mut(), root, 1, &fresh, Some(&fiote), || false)
            .unwrap_err()
            .contains("draft")
    );
    assert_eq!(session(&app, root).scene.as_ref(), Some(&draft));
    assert!(session(&app, root).dirty);
    assert_eq!(
        workspace_scene::open(&path).unwrap().revision,
        receipt.revision
    );
}

#[test]
fn later_measurement_wins_on_the_same_source_and_selection_change_retires_a_held_job() {
    let temp = tempfile::tempdir().unwrap();
    let (mut app, root) = super::super::tests::fixture();
    open_inspector(&mut app, root);
    text(
        &mut app,
        "Scene directory",
        temp.path().join("scene.workspace").to_str().unwrap(),
    );
    activate(&mut app, "New scene");
    text(&mut app, "New part name", "Bracket");
    activate(&mut app, "Create part and place it");
    activate(&mut app, "Place another copy of this part");
    activate(&mut app, "Save scene");
    activate(&mut app, "Accepted edits and measurements");
    let run = |app: &mut App, command| {
        let page = app
            .world_mut()
            .query_filtered::<Entity, With<Form>>()
            .single(app.world())
            .unwrap();
        Command::Accepted(command).apply(app.world_mut(), page);
    };
    run(&mut app, accepted::Action::Hold);
    activate(&mut app, "Measure distance from origin");
    activate(&mut app, "Finish the held measurement");
    assert!(
        session(&app, root)
            .message
            .contains("Discarded the late measurement")
    );
    run(&mut app, accepted::Action::Hold);
    let other = session(&app, root).scene.as_ref().unwrap().slots[0]
        .id
        .as_str()
        .to_owned();
    activate(&mut app, &format!("Placement {other}"));
    activate(&mut app, "Finish the held measurement");
    assert!(
        session(&app, root)
            .message
            .contains("Discarded the late measurement")
    );
    run(&mut app, accepted::Action::Hold);
    activate(&mut app, "Cancel the held measurement");
    assert!(session(&app, root).message.contains("Cancelled"));
}
