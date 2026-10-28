use super::*;
use super::super::tests::{fixture, visible};

fn chooser(world: &mut World, root: Entity, first: Entity) -> Entity {
    let target = capture(world, root, first, false).unwrap();
    open(world, target, false).unwrap();
    let owner = world.query_filtered::<Entity, With<Chooser>>().single(world).unwrap();
    visible(world, owner);
    owner
}

#[test]
fn chooser_cancel_preserves_the_model_and_confirm_performs_one_membership_commit() {
    let (mut world, root, first, second, _) = fixture();
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    let owner = chooser(&mut world, root, first);
    let id = world.get::<Managed>(second).unwrap().id;
    Edit::Choice(id).apply(&mut world, owner);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    Edit::Cancel.apply(&mut world, owner);
    assert!(!shortcuts::keyboard_owned(&world));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    let owner = chooser(&mut world, root, first);
    Edit::Choice(id).apply(&mut world, owner);
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get_entity(owner).is_err());
    assert!(!shortcuts::keyboard_owned(&world));
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.compact_groups().len(), 1);
    assert_eq!(model.items(), before.items());
    Edit::Confirm.apply(&mut world, owner);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop.compact_groups().len(), 1);
}

#[test]
fn a_changed_selected_surface_or_drag_keeps_the_draft_and_cannot_commit_to_a_replacement() {
    let (mut world, root, first, second, _) = fixture();
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    let owner = chooser(&mut world, root, first);
    let id = world.get::<Managed>(second).unwrap().id;
    Edit::Choice(id).apply(&mut world, owner);
    let surface = world.get::<ApplicationWindow>(second).unwrap().surface;
    world.get_mut::<ApplicationWindow>(second).unwrap().surface = SurfaceId(999);
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get::<Chooser>(owner).unwrap().choices.iter().any(|choice| choice.selected));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    world.get_mut::<ApplicationWindow>(second).unwrap().surface = surface;
    world.insert_resource(crate::host::input::DragActive(true));
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get::<Chooser>(owner).unwrap().message.contains("drag"));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
    Edit::Cancel.apply(&mut world, owner);
    assert!(!shortcuts::keyboard_owned(&world));
}

#[test]
fn a_replacement_modal_retains_keyboard_focus_when_the_old_chooser_receives_a_late_action() {
    let (mut world, root, first, _, _) = fixture();
    let before = world.get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    let owner = chooser(&mut world, root, first);
    let replacement = world.spawn((Node::default(), ChildOf(root))).id();
    shortcuts::own_keyboard(&mut world, replacement);
    world.resource_mut::<InputFocus>().set(replacement, FocusCause::Navigated);
    Edit::Confirm.apply(&mut world, owner);
    assert!(world.get_entity(owner).is_err());
    assert_eq!(world.resource::<shortcuts::KeyboardModalOwner>().0, Some(replacement));
    assert_eq!(world.resource::<InputFocus>().get(), Some(replacement));
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[0].desktop, before);
}
