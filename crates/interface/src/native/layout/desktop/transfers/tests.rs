use super::*;
use crate::native::actions::Action;

mod captured;
mod projection;
mod unmanaged;

fn fixture() -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.insert_resource(DefaultLayout(WorkspaceLayout::Tiled));
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(WorkspaceLayout::Tiled);
    crate::native::layout::engine::resolve(&mut world);
    focus_item(&mut world, application);
    (world, root, native, application)
}

fn destination(world: &mut World, root: Entity, mode: WorkspaceLayout) {
    let mut entry = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    entry.id = 2;
    entry.name = "Other".into();
    entry.desktop = Model::new(mode);
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(entry);
}

#[test]
fn identity_selection_rejects_a_closed_destination_and_creation_stays_staged_until_commit() {
    let (mut world, root, _, _) = fixture();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let prepared = prepare_identity(&world, root, 2).unwrap();
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .retain(|entry| entry.id != 2);
    assert!(prepared.commit(&mut world).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
    let creation = prepare_creation(&world, root).unwrap();
    let id = creation.destination();
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 1);
    creation.commit(&mut world).unwrap();
    let spaces = world.get::<Workspaces>(root).unwrap();
    assert_eq!(spaces.entries.len(), 2);
    assert_eq!(spaces.active, 1);
    assert_eq!(crate::native::workspace::numbers(&world, root), vec![1, id]);
}

fn model(world: &World, root: Entity, workspace: u64) -> &Model {
    &world
        .get::<Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|entry| entry.id == workspace)
        .unwrap()
        .desktop
}

#[test]
fn first_resolve_into_an_unvisited_destination_preserves_the_live_application_and_width() {
    let (mut world, root, native, application) = fixture();
    let id = world.get::<Managed>(application).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_item_width(id, crate::layout::desktop::Width::TwoThirds)
        .unwrap();
    crate::native::layout::engine::resolve(&mut world);
    let original = model(&world, root, 1)
        .items()
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .clone();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    assert!(!world.resource::<Runtime>().seen.contains_key(&(root, 2)));
    let prepared = prepare_move(&mut world, root, 2, TransferKind::Item).unwrap();
    let selection = prepared.commit(&mut world).unwrap().unwrap();
    assert!(selection.activate(&mut world));
    crate::native::layout::engine::resolve(&mut world);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 2);
    assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 1);
    assert_eq!(
        world.get::<ApplicationWindow>(application).unwrap().surface,
        SurfaceId(7)
    );
    assert_eq!(world.get::<Managed>(application).unwrap().id, id);
    assert_eq!(
        model(&world, root, 2).item_width(id),
        Some(crate::layout::desktop::Width::TwoThirds)
    );
    let moved = model(&world, root, 2)
        .items()
        .iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(moved.free, original.free);
    assert_eq!(moved.free_width, original.free_width);
    assert!(
        !model(&world, root, 1)
            .items()
            .iter()
            .any(|item| item.id == id)
    );
    let prepared = prepare_move(&mut world, root, 1, TransferKind::Item).unwrap();
    assert!(
        prepared
            .commit(&mut world)
            .unwrap()
            .unwrap()
            .activate(&mut world)
    );
    crate::native::layout::engine::resolve(&mut world);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
    assert_eq!(
        model(&world, root, 1).item_width(id),
        Some(crate::layout::desktop::Width::TwoThirds)
    );
    assert_eq!(
        world.get::<ApplicationWindow>(application).unwrap().surface,
        SurfaceId(7)
    );
}

#[test]
fn numbered_movement_creates_all_nine_destinations_and_same_workspace_is_a_noop() {
    let (mut world, root, _, application) = fixture();
    DesktopAction::MoveToWorkspace(9).apply(&mut world, root);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 9);
    let target = world.get::<Workspaces>(root).unwrap().entries[8].id;
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, target);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, target);
    crate::native::layout::engine::resolve(&mut world);
    let before = world.get::<CanvasView>(root).copied().unwrap();
    let input = world.resource::<bevy::input_focus::InputFocus>().get();
    let saved = model(&world, root, target).clone();
    DesktopAction::MoveToWorkspace(9).apply(&mut world, root);
    let current = world.get::<CanvasView>(root).unwrap();
    assert_eq!((current.center, current.zoom), (before.center, before.zoom));
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        input
    );
    assert_eq!(model(&world, root, target), &saved);
}

#[test]
fn missing_live_targets_and_stale_models_cannot_publish_partial_destinations() {
    let (mut world, root, _, application) = fixture();
    let prepared = prepare_numbered_move(&mut world, root, 9, TransferKind::Item).unwrap();
    let before = model(&world, root, 1).clone();
    world.despawn(application);
    assert!(prepared.commit(&mut world).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 1);
    assert_eq!(model(&world, root, 1), &before);
    let (mut world, root, native, application) = fixture();
    let prepared = prepare_numbered_move(&mut world, root, 9, TransferKind::Item).unwrap();
    let id = world.get::<Managed>(native).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_item_width(id, crate::layout::desktop::Width::Full)
        .unwrap();
    let changed = model(&world, root, 1).clone();
    assert!(prepared.commit(&mut world).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 1);
    assert_eq!(model(&world, root, 1), &changed);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
}

#[test]
fn native_editor_entity_and_text_remain_focused_after_a_move_to_a_free_workspace() {
    let (mut world, root, native, _) = fixture();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    let editor = world
        .spawn((
            crate::native::sand::single_line_editor(
                "kept text",
                world.resource::<crate::native::theme::Typography>(),
                0,
                128,
            ),
            ChildOf(native),
        ))
        .id();
    focus_item(&mut world, native);
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Navigated);
    destination(&mut world, root, WorkspaceLayout::Free);
    let prepared = prepare_move(&mut world, root, 2, TransferKind::Item).unwrap();
    assert!(
        prepared
            .commit(&mut world)
            .unwrap()
            .unwrap()
            .activate(&mut world)
    );
    crate::native::layout::engine::resolve(&mut world);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(editor)
    );
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value(),
        "kept text"
    );
    assert!(focus::following(&world, root));
    assert_eq!(world.get::<WorkspaceMember>(native).unwrap().0, 2);
}

#[test]
fn parent_owned_dialogs_move_with_the_row_and_retain_parent_fullscreen_and_dialog_focus() {
    let (mut world, root, native, application) = fixture();
    let dialog = world
        .spawn((
            CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size: Vec2::new(180.0, 120.0),
            },
            LayoutBox::new(Vec2::new(180.0, 120.0)),
            WorkspaceMember(1),
            ChildOf(root),
            ApplicationWindow {
                surface: SurfaceId(8),
                content_size: [180, 120],
            },
            TransientParent(application),
        ))
        .id();
    crate::native::layout::engine::resolve(&mut world);
    let parent = world.get::<Managed>(application).unwrap().id;
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .request_state(
            parent,
            crate::host::WindowRequest {
                fullscreen: Some(true),
                ..default()
            },
        );
    focus_item(&mut world, dialog);
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let prepared = prepare_move(&mut world, root, 2, TransferKind::Row).unwrap();
    assert!(
        prepared
            .commit(&mut world)
            .unwrap()
            .unwrap()
            .activate(&mut world)
    );
    crate::native::layout::engine::resolve(&mut world);
    for entity in [native, application, dialog] {
        assert_eq!(world.get::<WorkspaceMember>(entity).unwrap().0, 2);
    }
    assert_eq!(world.get::<TransientParent>(dialog).unwrap().0, application);
    assert_eq!(model(&world, root, 2).fullscreen(), Some(parent));
    assert_eq!(
        model(&world, root, 2).focused(),
        Some(world.get::<Managed>(dialog).unwrap().id)
    );
    assert!(model(&world, root, 1).items().is_empty());
}

#[test]
fn duplicate_live_placement_in_an_unvisited_destination_is_rejected() {
    let (mut world, root, _, application) = fixture();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let layout = *world.get::<LayoutBox>(application).unwrap();
    world.spawn((
        CanvasItem {
            position: bevy::math::DVec2::ZERO,
            size: Vec2::new(300.0, 200.0),
        },
        layout,
        WorkspaceMember(2),
        ChildOf(root),
    ));
    let source = model(&world, root, 1).clone();
    let target = model(&world, root, 2).clone();
    assert!(prepare_move(&mut world, root, 2, TransferKind::Item).is_err());
    assert_eq!(model(&world, root, 1), &source);
    assert_eq!(model(&world, root, 2), &target);
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 1);
}

#[test]
fn numeric_destinations_use_the_visible_order_and_respect_reserved_workspace_ids() {
    let (mut world, root, _, application) = fixture();
    let mut removed = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    removed.id = 30;
    world.get_mut::<Workspaces>(root).unwrap().trash.push(
        crate::native::workspace::TrashedWorkspace {
            workspace: removed,
            deleted_at: 1,
        },
    );
    let prepared = prepare_numbered_move(&mut world, root, 2, TransferKind::Item).unwrap();
    assert_eq!(prepared.destination(), 31);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 1);
    assert!(
        prepared
            .commit(&mut world)
            .unwrap()
            .unwrap()
            .activate(&mut world)
    );
    assert_eq!(world.get::<WorkspaceMember>(application).unwrap().0, 31);
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries[1].id, 31);
    assert!(prepare_selection(&world, root, 0).is_err());
    assert!(prepare_selection(&world, root, u32::MAX).is_err());
}

#[test]
fn explicit_unfocus_does_not_select_an_item_or_create_a_move_destination() {
    let (mut world, root, _, _) = fixture();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .clear_focus();
    let view = world.get::<CanvasView>(root).copied().unwrap();
    assert!(prepare_numbered_move(&mut world, root, 9, TransferKind::Item).is_err());
    assert_eq!(world.get::<Workspaces>(root).unwrap().entries.len(), 1);
    let current = world.get::<CanvasView>(root).unwrap();
    assert_eq!((current.center, current.zoom), (view.center, view.zoom));
    assert_eq!(model(&world, root, 1).focused(), None);
}
