use super::*;

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .add_message::<crate::cell_bridge::CellMessage>()
        .add_systems(Update, receive);
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let owner = super::super::spawn(app.world_mut(), root, 1, DVec2::ZERO, Default::default());
    (app, owner)
}

#[cfg_attr(test, test)]
fn timeout_releases_import_controls_and_ignores_a_late_result() {
    let (mut app, owner) = fixture();
    let mut state = Import::default();
    state.request = Some("submitted".into());
    state.committing = true;
    state.started = Some(std::time::Instant::now() - std::time::Duration::from_secs(31));
    app.world_mut().entity_mut(owner).insert(state);
    app.update();
    let state = app.world().get::<Import>(owner).unwrap();
    assert!(state.request.is_none());
    assert!(state.message.contains("may still finish"));
    app.world_mut()
        .write_message(crate::cell_bridge::CellMessage(ServerMessage::ActionOk {
            id: "submitted".into(),
            created: None,
            warnings: Vec::new(),
            facts: 0,
            data: Some(serde_json::json!({"created":1,"reused":0})),
        }));
    app.update();
    assert!(!app.world().get::<Import>(owner).unwrap().imported);
    ImportAction::Cancel.apply(app.world_mut(), owner);
    assert!(app.world().get::<Import>(owner).is_none());
}

#[cfg_attr(test, test)]
fn cancelling_a_preview_is_local_and_submitted_import_is_disclosed() {
    let (mut app, owner) = fixture();
    let mut state = Import::default();
    state.fingerprint = Some("preview".into());
    app.world_mut().entity_mut(owner).insert(state);
    ImportAction::Cancel.apply(app.world_mut(), owner);
    assert!(app.world().get::<Import>(owner).is_none());
    let mut state = Import::default();
    state.committing = true;
    app.world_mut().entity_mut(owner).insert(state);
    ImportAction::Cancel.apply(app.world_mut(), owner);
    assert!(
        app.world()
            .resource::<crate::notifications::Notifications>()
            .log
            .snapshot()
            .1
            .iter()
            .any(|notice| notice.message.contains("does not cancel"))
    );
}

crate::laboratory_cases! {
    timeout_releases_import_controls_and_ignores_a_late_result,
    cancelling_a_preview_is_local_and_submitted_import_is_disclosed,
}
