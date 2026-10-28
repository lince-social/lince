use super::*;
use bevy::math::DVec2;

pub(super) fn fixture() -> (App, Entity, Entity, Entity) {
    let (mut app, root) = crate::native::edit_mode::tests::fixture();
    app.world_mut().entity_mut(root).insert(ComputedNode {
        size: Vec2::new(1000.0, 700.0),
        inverse_scale_factor: 1.0,
        ..default()
    });
    let item = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(400.0, -200.0),
                size: Vec2::new(300.0, 200.0),
            },
            LayoutBox::new(Vec2::new(300.0, 200.0)),
            WorkspaceMember(1),
            Name::new("Working note"),
            ChildOf(root),
        ))
        .id();
    let editor = app
        .world_mut()
        .spawn((
            crate::native::sand::editable("Retained draft"),
            ChildOf(item),
        ))
        .id();
    app.world_mut()
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(vec![item]));
    DesktopAction::ToggleFocus.apply(app.world_mut(), root);
    focus::resolve(app.world_mut());
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    (app, root, item, editor)
}

#[test]
fn opening_and_cancelling_preserve_layout_dimensions_draft_and_current_frame_follow() {
    let (mut app, root, item, editor) = fixture();
    let geometry = *app.world().get::<CanvasItem>(item).unwrap();
    let view = *app.world().get::<CanvasView>(root).unwrap();
    let mode = app.world().get::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .mode();
    DesktopAction::ToggleOverview.apply(app.world_mut(), root);
    assert!(active(app.world()));
    assert!(!focus::following(app.world(), root));
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .mode(),
        mode
    );
    app.world_mut()
        .get_mut::<CanvasItem>(item)
        .unwrap()
        .position += DVec2::new(83.0, -47.0);
    focus::resolve(app.world_mut());
    assert!(app.world().get::<CanvasView>(root).unwrap().center == view.center);
    DesktopAction::ToggleOverview.apply(app.world_mut(), root);
    focus::resolve(app.world_mut());
    assert!(!active(app.world()));
    assert!(focus::following(app.world(), root));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert_eq!(
        app.world().get::<CanvasView>(root).unwrap().center,
        app.world().get::<CanvasItem>(item).unwrap().position
    );
    assert_eq!(
        app.world().get::<CanvasItem>(item).unwrap().size,
        geometry.size
    );
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Retained draft"
    );
    assert!(app.world().resource::<Previews>().0.is_empty());
}

#[test]
fn overview_owns_input_but_only_its_registered_toggle_remains_a_desktop_shortcut() {
    let (mut app, root, _, _) = fixture();
    DesktopAction::ToggleOverview.apply(app.world_mut(), root);
    let bindings = crate::native::shortcuts::active_desktop_bindings(app.world());
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].action, DesktopAction::ToggleOverview);
    assert_eq!(bindings[0].key, KeyCode::Space);
    let different = app.world_mut().spawn_empty().id();
    crate::native::shortcuts::own_keyboard(app.world_mut(), different);
    assert!(crate::native::shortcuts::active_desktop_bindings(app.world()).is_empty());
    crate::native::shortcuts::release_keyboard(app.world_mut(), different);
    close(app.world_mut(), true);
    assert!(crate::native::shortcuts::active_desktop_bindings(app.world()).len() > 1);
}

#[test]
fn cancelling_restores_a_live_edit_panel_field_without_losing_the_window_focus_mode() {
    let (mut app, root, _, _) = fixture();
    let panel = app
        .world()
        .get::<crate::native::edit_mode::EditMode>(root)
        .unwrap()
        .panel;
    app.world_mut().get_mut::<Node>(panel).unwrap().display = Display::Flex;
    let editor = app
        .world_mut()
        .spawn((
            crate::native::sand::editable("Unsaved settings"),
            ChildOf(panel),
        ))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    toggle(app.world_mut(), root);
    close(app.world_mut(), true);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert!(focus::following(app.world(), root));
    assert_eq!(
        app.world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Unsaved settings"
    );
}

#[test]
fn stale_selection_cannot_close_the_modal_or_restore_input_to_a_minimized_or_destroyed_target() {
    let (mut app, root, item, editor) = fixture();
    let identity = app.world().get::<LayoutBox>(item).unwrap().id;
    DesktopAction::ToggleOverview.apply(app.world_mut(), root);
    app.world_mut().despawn(item);
    assert!(!activate(app.world_mut(), root, 1, Some(identity)));
    assert!(active(app.world()));
    close(app.world_mut(), true);
    assert_ne!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert!(!focus::following(app.world(), root));
}

#[test]
fn workspace_selection_uses_existing_identities_and_does_not_move_or_recreate_items() {
    let (mut app, root, item, _) = fixture();
    let layout = *app.world().get::<LayoutBox>(item).unwrap();
    let mut second = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    second.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Free);
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    app.world_mut().entity_mut(item).insert(WorkspaceMember(2));
    DesktopAction::ToggleOverview.apply(app.world_mut(), root);
    assert!(activate(app.world_mut(), root, 2, Some(layout.id)));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 2);
    assert_eq!(app.world().get::<LayoutBox>(item).unwrap().id, layout.id);
    assert_eq!(app.world().get::<ChildOf>(item).unwrap().parent(), root);
    assert!(focus::following(app.world(), root));
    assert!(!active(app.world()));
}

#[test]
fn navigation_uses_visible_horizontal_order_and_workspace_names_refresh_without_reopening() {
    let (mut app, root, right, _) = fixture();
    let left = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(-400.0, -200.0),
                size: Vec2::new(300.0, 200.0),
            },
            LayoutBox::new(Vec2::new(300.0, 200.0)),
            WorkspaceMember(1),
            Name::new("Left note"),
            ChildOf(root),
        ))
        .id();
    let left_id = app.world().get::<LayoutBox>(left).unwrap().id;
    let right_id = app.world().get::<LayoutBox>(right).unwrap().id;
    toggle(app.world_mut(), root);
    let overview = app.world().resource::<Overview>().0.as_ref().unwrap();
    assert_eq!(overview.cards[0].order, vec![left_id, right_id]);
    navigate(app.world_mut(), Direction::Left);
    assert_eq!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .selected,
        (1, Some(left_id))
    );
    let title = app.world().resource::<Overview>().0.as_ref().unwrap().cards[0].title;
    app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0].name =
        "Renamed workspace".into();
    update(app.world_mut());
    assert_eq!(
        app.world().resource::<Overview>().0.as_ref().unwrap().cards[0].title,
        title
    );
    let caption = app.world().get::<Children>(title).unwrap()[0];
    assert!(
        app.world()
            .get::<Text>(caption)
            .unwrap()
            .0
            .contains("Renamed workspace")
    );
    app.world_mut().despawn(left);
    update(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .selected,
        (1, None)
    );
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(title));
}
