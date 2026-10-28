use super::*;

fn fixture() -> (World, Entity, Entity, [u8; 16], String) {
    let mut world = World::new();
    world.init_resource::<Windows>();
    world.insert_resource(crate::native::theme::Typography(Handle::default()));
    let root = world.spawn((Workspaces::default(), Node::default())).id();
    let placement = [19; 16];
    let uid = nucleus::new_uid("r");
    super::panel(&mut world, root, root, Target::Window { placement, uid: uid.clone() });
    let page = world.query_filtered::<Entity, With<Page>>().single(&world).unwrap();
    let visible: Vec<_> = world.query_filtered::<Entity, With<Node>>().iter(&world).collect();
    for entity in visible { world.entity_mut(entity).insert(InheritedVisibility::VISIBLE); }
    publish_windows(&mut world, true, vec![WindowPolicy { placement, uid: uid.clone(), excluded: false }]);
    refresh(&mut world);
    (world, root, page, placement, uid)
}

#[test]
fn exclusion_edit_keeps_its_captured_profile_and_preserves_a_failed_draft_for_retry() {
    let (mut world, root, page, placement, uid) = fixture();
    Change::Toggle.apply(&mut world, page);
    Change::Save.apply(&mut world, page);
    let edits = take_edits(&mut world);
    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    assert_eq!((edit.owner, edit.placement, &edit.uid, edit.excluded), (root, placement, &uid, true));
    assert!(validate_edit(&world, edit).is_ok());
    complete_edit(&mut world, edit, Err("The profile path is not writable.".into()));
    assert!(world.get::<Page>(page).unwrap().draft);
    assert_eq!(value(&world, &world.get::<Page>(page).unwrap().target), Some(false));
    Change::Save.apply(&mut world, page);
    let retry = take_edits(&mut world).pop().unwrap();
    assert_ne!(retry.request, edit.request);
    publish_windows(&mut world, true, vec![WindowPolicy { placement, uid, excluded: true }]);
    complete_edit(&mut world, &retry, Ok(()));
    assert_eq!(world.get::<Page>(page).unwrap().origin, Some(true));
    assert!(world.get::<Page>(page).unwrap().pending.is_none());
}

#[test]
fn concurrent_profile_changes_require_refresh_and_cancel_does_not_write() {
    let (mut world, _, page, placement, uid) = fixture();
    Change::Toggle.apply(&mut world, page);
    publish_windows(&mut world, true, vec![WindowPolicy { placement, uid, excluded: true }]);
    Change::Save.apply(&mut world, page);
    assert!(take_edits(&mut world).is_empty());
    assert!(world.get::<Page>(page).unwrap().draft);
    Change::Cancel.apply(&mut world, page);
    assert_eq!(world.get::<Page>(page).unwrap().origin, Some(true));
    assert!(take_edits(&mut world).is_empty());
}

#[test]
fn queued_edits_cannot_outlive_their_original_canvas_or_control() {
    let (mut world, root, page, _, _) = fixture();
    Change::Toggle.apply(&mut world, page);
    Change::Save.apply(&mut world, page);
    let edit = take_edits(&mut world).pop().unwrap();
    world.get_mut::<Workspaces>(root).unwrap().canvas_id = nucleus::new_uid("canvas");
    assert!(validate_edit(&world, &edit).is_err());
    world.despawn(page);
    assert!(validate_edit(&world, &edit).is_err());
    complete_edit(&mut world, &edit, Err("The original control was closed.".into()));
    assert!(world.get_entity(page).is_err());
}
