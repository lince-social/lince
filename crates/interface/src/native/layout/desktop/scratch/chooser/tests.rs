use super::*;

fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    focus_item(&mut world, application);
    checked(&mut world, root, DesktopAction::MarkScratch).unwrap();
    (world, root, native, application)
}

fn owner(world: &mut World) -> Entity {
    world.query_filtered::<Entity, With<Chooser>>().iter(world).next().unwrap()
}

fn visible(world: &mut World, mut entity: Entity) {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        world.entity_mut(entity).insert(InheritedVisibility::VISIBLE);
        let Some(parent) = world.get::<ChildOf>(entity) else { return; };
        entity = parent.parent();
    }
}

#[test]
fn chooser_cancel_keeps_membership_focus_and_content_and_confirm_moves_only_the_captured_item() {
    let (mut world, root, native, _) = fixture();
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    assert!(checked(&mut world, root, DesktopAction::ShowScratch).unwrap());
    let owner = owner(&mut world);
    visible(&mut world, owner);
    Edit::Choose(0).apply(&mut world, owner);
    Edit::Cancel.apply(&mut world, owner);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    assert!(world.get_entity(owner).is_err());
    assert!(checked(&mut world, root, DesktopAction::ShowScratch).unwrap());
    let owner = self::owner(&mut world);
    visible(&mut world, owner);
    let index = world.get::<Chooser>(owner).unwrap().choices.iter().position(|(target, _)| target.entity == native).unwrap();
    Edit::Choose(index).apply(&mut world, owner);
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get_entity(owner).is_err());
    let placement = PlacementId(world.get::<LayoutBox>(native).unwrap().id);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.focused(), Some(placement));
    assert_eq!(world.resource::<InputFocus>().get(), Some(native));
    assert!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.items().iter().find(|item| item.id == placement).unwrap().floating);
    Edit::Confirm.apply(&mut world, owner);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.items().iter().filter(|item| item.id == placement).count(), 1);
}

#[test]
fn stale_choice_preserves_the_draft_and_session_loss_retires_confirmation() {
    let (mut world, root, _, application) = fixture();
    checked(&mut world, root, DesktopAction::ShowScratch).unwrap();
    let owner = owner(&mut world);
    visible(&mut world, owner);
    let index = world.get::<Chooser>(owner).unwrap().choices.iter().position(|(target, _)| target.entity == application).unwrap();
    Edit::Choose(index).apply(&mut world, owner);
    let original = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    world.get_mut::<ApplicationWindow>(application).unwrap().surface = SurfaceId(99);
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get::<Chooser>(owner).is_some_and(|state| state.chosen == Some(index) && !state.message.is_empty()));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, original);
    super::super::cancel(&mut world);
    assert!(world.get_entity(owner).is_err());
    Edit::Confirm.apply(&mut world, owner);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, original);
}

#[test]
fn replacing_the_keyboard_owner_keeps_the_new_dialog_and_private_labels_keep_their_source() {
    let (mut world, root, native, _) = fixture();
    world.get_mut::<LayoutBox>(native).unwrap().capture_excluded = true;
    checked(&mut world, root, DesktopAction::ShowScratch).unwrap();
    let owner = owner(&mut world);
    let button = world.get::<Chooser>(owner).unwrap().choices.iter().find(|(target, _)| target.entity == native).unwrap().1.0;
    assert!(crate::native::capture::excluded(&world, button));
    let replacement = world.spawn(Node::default()).id();
    shortcuts::own_keyboard(&mut world, replacement);
    refresh(&mut world);
    assert!(world.get_entity(owner).is_err());
    assert_eq!(world.resource::<shortcuts::KeyboardModalOwner>().0, Some(replacement));
}
