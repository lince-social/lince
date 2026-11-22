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
