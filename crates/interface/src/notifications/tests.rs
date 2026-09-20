use super::*;
use crate::{
    edit_mode::{EditAction, EditControl, EditMode},
    icons::IconButton,
};
use bevy::ui_widgets::Activate;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn fixture(log: cell::Diagnostics) -> (App, Entity) {
    let (mut app, root) = crate::edit_mode::tests::fixture();
    app.insert_resource(Notifications::new(log))
        .add_plugins(NotificationsPlugin);
    app.update();
    (app, root)
}

fn click(app: &mut App, label: &str) {
    let button = app
        .world_mut()
        .query::<(Entity, &IconButton)>()
        .iter(app.world())
        .find(|(_, icon)| icon.label == label)
        .unwrap()
        .0;
    app.world_mut().trigger(Activate { entity: button });
    app.update();
    app.update();
}

fn toasts(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<NotificationToast>>()
        .iter(app.world())
        .collect()
}

#[cfg_attr(test, test)]
fn workspace_load_error_is_a_startup_toast_and_keeps_saved_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    std::fs::write(&path, b"broken").unwrap();
    let log = cell::Diagnostics::default();
    let message =
        "Could not load workspaces: no valid workspace snapshot. Saved data has been kept.";
    log.report("interface::workspaces", message);
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .insert_resource(Notifications::new(log.clone()))
        .insert_resource(crate::workspace::WorkspaceFile::new(path.clone()))
        .add_plugins((
            crate::workspace::WorkspacePlugin,
            crate::edit_mode::EditModePlugin,
            NotificationsPlugin,
        ));
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    app.update();
    assert_eq!(toasts(&mut app).len(), 1);
    assert_eq!(log.snapshot().1[0].message, message);
    assert_eq!(log.snapshot().1[0].occurrences, 2);
    click(&mut app, "Close notification toast");
    assert!(toasts(&mut app).is_empty());
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == message)
    );
    assert_eq!(log.snapshot().1.len(), 1);
    assert_eq!(log.snapshot().1[0].occurrences, 2);
    click(&mut app, "Notifications (1)");
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == message)
    );
    click(&mut app, "Delete notification");
    assert!(log.snapshot().1.is_empty());
    assert!(toasts(&mut app).is_empty());
    assert!(
        app.world()
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .error
            .is_some()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"broken");
}

#[cfg_attr(test, test)]
fn recommendations_distinguish_local_connections_storage_and_desktop_problems() {
    assert!(recommendation("interface::connection", "Connection closed").contains("this Cell"));
    assert!(recommendation("cell", "No space left on device").contains("disk space"));
    assert!(recommendation("cell", "Permission denied").contains("your account"));
    assert!(recommendation("lince_interface::tray", "Unavailable").contains("keep using Lince"));
}

#[cfg_attr(test, test)]
fn toast_close_despawns_without_deleting_history_or_reappearing_on_unrelated_arrivals() {
    let log = cell::Diagnostics::default();
    let (mut app, root) = fixture(log.clone());
    log.report("cell", "Could not save the document");
    app.update();
    assert!(!app.world().get::<EditMode>(root).unwrap().enabled);
    let toast = toasts(&mut app)[0];
    let stack = app.world().get::<ChildOf>(toast).unwrap().parent();
    let children: Vec<_> = app.world().get::<Children>(toast).unwrap().iter().collect();
    let id = log.snapshot().1[0].id;
    let badge = app
        .world_mut()
        .query::<&NotificationBadge>()
        .single(app.world())
        .unwrap()
        .0;
    assert_eq!(app.world().get::<Text>(badge).unwrap().0, "1");
    click(&mut app, "Close notification toast");
    assert_eq!(app.world().get::<Text>(badge).unwrap().0, "1");
    assert!(app.world().get_entity(toast).is_err());
    assert!(app.world().get_entity(stack).is_err());
    assert!(
        children
            .iter()
            .all(|entity| app.world().get_entity(*entity).is_err())
    );
    assert_eq!(log.snapshot().1.len(), 1);
    assert!(toasts(&mut app).is_empty());
    log.report("cell", "Connection lost");
    app.update();
    assert_eq!(toasts(&mut app).len(), 1);
    assert!(
        app.world_mut()
            .query::<&NotificationToast>()
            .iter(app.world())
            .all(|toast| toast.id != id)
    );
    log.report("cell", "Could not save the document");
    app.update();
    assert_eq!(toasts(&mut app).len(), 2);
    assert_eq!(app.world().get::<Text>(badge).unwrap().0, "2");
    assert!(
        app.world_mut()
            .query::<&NotificationToast>()
            .iter(app.world())
            .any(|toast| toast.id == id && toast.occurrences == 2)
    );
}

#[cfg_attr(test, test)]
fn notifications_button_precedes_edit_and_panel_deletes_one_or_all() {
    let log = cell::Diagnostics::default();
    let (mut app, root) = fixture(log.clone());
    let edit = app.world().get::<EditMode>(root).unwrap().toggle;
    let bell = app.world().get::<NotificationCenter>(root).unwrap().button;
    let toolbar = app.world().get::<ChildOf>(edit).unwrap().parent();
    let children: Vec<_> = app
        .world()
        .get::<Children>(toolbar)
        .unwrap()
        .iter()
        .collect();
    let index = children.iter().position(|entity| *entity == edit).unwrap();
    assert_eq!(children[index - 1], bell);
    assert!(app.world().get::<EditControl>(bell).is_none());
    log.report("cell", "First");
    log.report("cell", "Second");
    app.update();
    EditAction::Open.apply(app.world_mut(), root);
    click(&mut app, "Notifications (2)");
    assert!(!app.world().get::<EditMode>(root).unwrap().enabled);
    let drawer = app
        .world()
        .get::<NotificationCenter>(root)
        .unwrap()
        .panel
        .unwrap();
    assert_eq!(app.world().get::<ChildOf>(drawer).unwrap().parent(), root);
    click(&mut app, "Delete notification");
    assert_eq!(log.snapshot().1.len(), 1);
    assert_eq!(toasts(&mut app).len(), 1);
    let deleted = log.snapshot().1[0].id;
    click(&mut app, "Close notifications");
    assert!(app.world().get_entity(drawer).is_err());
    assert_eq!(log.snapshot().1[0].id, deleted);
    click(&mut app, "Notifications (1)");
    click(&mut app, "Delete all notifications");
    assert!(log.snapshot().1.is_empty());
    assert_eq!(
        app.world_mut()
            .query::<&NotificationBadge>()
            .iter(app.world())
            .count(),
        0
    );
    assert!(toasts(&mut app).is_empty());
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "No notifications.")
    );
    let drawer = app
        .world()
        .get::<NotificationCenter>(root)
        .unwrap()
        .panel
        .unwrap();
    EditAction::Open.apply(app.world_mut(), root);
    assert!(app.world().get_entity(drawer).is_err());
    assert!(app.world().get::<EditMode>(root).unwrap().enabled);
}

#[cfg_attr(test, test)]
fn notifications_wake_once_and_preserve_idle_toast_entities() {
    let log = cell::Diagnostics::default();
    let (mut app, _) = fixture(log.clone());
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    app.insert_resource(WakeSignal::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    app.update();
    let before = wakes.load(Ordering::Relaxed);
    let incoming = log.clone();
    std::thread::spawn(move || incoming.report("cell", "Could not save"))
        .join()
        .unwrap();
    assert!(wakes.load(Ordering::Relaxed) > before);
    log.report("cell", "Could not save");
    app.update();
    let toasts_before = toasts(&mut app);
    assert_eq!(toasts_before.len(), 1);
    let count = app.world().entities().len();
    app.update();
    assert_eq!(toasts(&mut app), toasts_before);
    assert_eq!(app.world().entities().len(), count);
    assert!(
        !app.world()
            .entity(toasts_before[0])
            .get_ref::<Node>()
            .unwrap()
            .is_changed()
    );
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Seen 2 times")
    );
}

#[cfg_attr(test, test)]
fn closing_focused_toast_restores_focus_and_panel_escape_keeps_history() {
    use bevy::input_focus::{FocusCause, InputFocus};
    let log = cell::Diagnostics::default();
    let (mut app, root) = fixture(log.clone());
    log.report("cell", "New notification");
    app.update();
    let button = app
        .world_mut()
        .query::<(Entity, &IconButton)>()
        .iter(app.world())
        .find(|(_, icon)| icon.label == "Close notification toast")
        .unwrap()
        .0;
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(button, FocusCause::Navigated);
    click(&mut app, "Close notification toast");
    let bell = app.world().get::<NotificationCenter>(root).unwrap().button;
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(bell));
    click(&mut app, "Notifications (1)");
    let drawer = app
        .world()
        .get::<NotificationCenter>(root)
        .unwrap()
        .panel
        .unwrap();
    let bindings = app
        .world()
        .get::<crate::actions::KeyBindings>(drawer)
        .unwrap();
    let escape = bindings
        .0
        .iter()
        .find(|binding| binding.key == KeyCode::Escape)
        .unwrap()
        .actions
        .clone();
    escape.run(app.world_mut(), drawer);
    assert!(app.world().get_entity(drawer).is_err());
    assert_eq!(log.snapshot().1.len(), 1);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(bell));
}

#[cfg_attr(test, test)]
fn saved_history_stays_in_panel_and_toast_close_does_not_change_journal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notifications.json");
    let log = cell::Diagnostics::default();
    let journal = cell::DiagnosticJournal::open(path.clone(), log.clone()).unwrap();
    log.report("cell", "Saved notice");
    let (mut app, _) = fixture(log.clone());
    assert!(toasts(&mut app).is_empty());
    log.report("cell", "New notice");
    app.update();
    click(&mut app, "Close notification toast");
    drop(journal);
    let restored = cell::Diagnostics::default();
    let journal = cell::DiagnosticJournal::open(path.clone(), restored.clone()).unwrap();
    assert_eq!(restored.snapshot().1.len(), 2);
    drop(journal);
    let journal = cell::DiagnosticJournal::open(path.clone(), log.clone()).unwrap();
    click(&mut app, "Notifications (2)");
    click(&mut app, "Delete all notifications");
    drop(journal);
    let restored = cell::Diagnostics::default();
    let _journal = cell::DiagnosticJournal::open(path, restored.clone()).unwrap();
    assert!(restored.snapshot().1.is_empty());
}

crate::laboratory_cases! {
    workspace_load_error_is_a_startup_toast_and_keeps_saved_data,
    recommendations_distinguish_local_connections_storage_and_desktop_problems,
    toast_close_despawns_without_deleting_history_or_reappearing_on_unrelated_arrivals,
    notifications_button_precedes_edit_and_panel_deletes_one_or_all,
    notifications_wake_once_and_preserve_idle_toast_entities,
    closing_focused_toast_restores_focus_and_panel_escape_keeps_history,
    saved_history_stays_in_panel_and_toast_close_does_not_change_journal,
}
