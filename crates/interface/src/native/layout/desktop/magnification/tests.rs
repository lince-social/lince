use super::*;
use crate::host::{OutputViewport, Rectangle, magnification::{Head, Response}, raster::RasterMapping};

fn fixture(available: bool) -> (App, Entity, std::sync::mpsc::Receiver<HostRequest>) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>().init_resource::<crate::native::theme::Typography>().init_resource::<InputFocus>();
    install(&mut app);
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    app.insert_resource(HostRequests(sender));
    let output = |id, workspace, x| {
        let bounds = Rectangle::new([x, 0.0], [800.0, 600.0]).unwrap();
        OutputViewport { rotation: Default::default(), id: OutputId(id), bounds, usable: Some(bounds), physical_size: [800, 600], raster: RasterMapping { origin: bounds.origin(), scale: [1.0; 2] }, workspace }
    };
    let first = output(1, 1, 0.0);
    let second = output(2, 2, 800.0);
    let mut spaces = Workspaces::default();
    let mut other = spaces.entries[0].clone(); other.id = 2; spaces.entries.push(other);
    let root = app.world_mut().spawn((Node::default(), spaces, HostViewport(first.clone()), InheritedVisibility::VISIBLE)).id();
    app.insert_resource(crate::native::output_views::OutputViews::new(vec![first, second], OutputId(1)).unwrap());
    app.insert_resource(Snapshot { available, revision: 1, heads: vec![
        Head { output: OutputId(1), name: "DP-1".into(), requested: 1.0, effective: Some(1.0) },
        Head { output: OutputId(2), name: "DP-2".into(), requested: 1.5, effective: Some(1.5) },
    ], ..default() });
    panel(app.world_mut(), root, root);
    settle(&mut app);
    let page = app.world_mut().query_filtered::<Entity, With<Page>>().single(app.world()).unwrap();
    Change::Toggle.apply(app.world_mut(), page);
    settle(&mut app);
    (app, page, receiver)
}

fn settle(app: &mut App) {
    let nodes = app.world_mut().query_filtered::<Entity, With<Node>>().iter(app.world()).collect::<Vec<_>>();
    for node in nodes { app.world_mut().entity_mut(node).insert(InheritedVisibility::VISIBLE); }
    let mut fields = app.world_mut().query::<&mut EditableText>();
    for mut field in fields.iter_mut(app.world_mut()) { field.pending_edits.clear(); }
    app.update();
}

fn value(app: &mut App, field: Entity, value: &str) {
    let mut input = app.world_mut().get_mut::<EditableText>(field).unwrap();
    input.editor.set_text(value);
    input.pending_edits.clear();
}

#[test]
fn drafts_cancel_and_save_against_the_original_output_and_saved_revision() {
    let (mut app, page, receiver) = fixture(true);
    let field = app.world().get::<Page>(page).unwrap().factor;
    value(&mut app, field, "2.5");
    Change::Select(OutputId(2)).apply(app.world_mut(), page);
    assert_eq!(app.world().get::<Page>(page).unwrap().selected, Some(OutputId(1)));
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::Magnification(request) = receiver.try_recv().unwrap() else { panic!("magnification request"); };
    assert_eq!(request.output, OutputId(1));
    assert_eq!(request.factor, 2.5);
    assert_eq!(request.revision, 1);
    assert_eq!(request.context.output, Some(1));
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    {
        let mut snapshot = app.world_mut().resource_mut::<Snapshot>();
        snapshot.revision = 2;
        snapshot.heads[0].requested = 2.5;
        snapshot.response = Some(Response { request: request.id, outcome: Outcome::Saved { warning: None } });
    }
    settle(&mut app);
    value(&mut app, field, "4");
    Change::Cancel.apply(app.world_mut(), page);
    settle(&mut app);
    assert_eq!(read(app.world(), field).unwrap(), 2.5);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn validation_external_changes_and_output_loss_preserve_drafts_until_an_explicit_reload() {
    let (mut app, page, receiver) = fixture(true);
    let field = app.world().get::<Page>(page).unwrap().factor;
    for invalid in ["0", "5", "NaN", "inf", "invalid"] {
        value(&mut app, field, invalid);
        Change::Save.apply(app.world_mut(), page);
        assert!(receiver.try_recv().is_err());
    }
    value(&mut app, field, "2");
    app.world_mut().resource_mut::<Snapshot>().revision = 2;
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert_eq!(read(app.world(), field).unwrap(), 2.0);
    Change::Reload.apply(app.world_mut(), page);
    let HostRequest::MagnificationReload(request) = receiver.try_recv().unwrap() else { panic!("magnification reload"); };
    {
        let mut snapshot = app.world_mut().resource_mut::<Snapshot>();
        snapshot.heads[0].requested = 3.0;
        snapshot.response = Some(Response { request: request.id, outcome: Outcome::Loaded });
    }
    settle(&mut app);
    assert_eq!(read(app.world(), field).unwrap(), 3.0);
    value(&mut app, field, "4");
    app.world_mut().resource_mut::<Snapshot>().heads.remove(0);
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert_eq!(read(app.world(), field).unwrap(), 4.0);
}

#[test]
fn changed_canvas_and_pending_paste_cannot_save_or_retarget_a_draft() {
    let (mut app, page, receiver) = fixture(true);
    let state = app.world().get::<Page>(page).unwrap();
    let root = state.root;
    let field = state.factor;
    value(&mut app, field, "2");
    app.world_mut().get_mut::<EditableText>(field).unwrap().pending_paste = Some(bevy::clipboard::ClipboardRead::Pending(Default::default()));
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    Change::Cancel.apply(app.world_mut(), page);
    settle(&mut app);
    assert!(app.world().get::<EditableText>(field).unwrap().pending_paste.is_none());
    value(&mut app, field, "2");
    app.world_mut().get_mut::<Workspaces>(root).unwrap().canvas_id.push_str("-replacement");
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert_eq!(read(app.world(), field).unwrap(), 2.0);
}

#[test]
fn registered_adjustments_start_unbound_and_use_one_captured_request() {
    let (mut app, page, receiver) = fixture(true);
    let root = app.world().get::<Page>(page).unwrap().root;
    for shortcut in [shortcuts::Shortcut::MagnifyIncrease, shortcuts::Shortcut::MagnifyDecrease, shortcuts::Shortcut::MagnifyReset] {
        assert_eq!(shortcut.default_binding(), "Unbound");
    }
    adjust(app.world_mut(), root, Adjustment::Increase);
    let HostRequest::Magnification(request) = receiver.try_recv().unwrap() else { panic!("magnification command"); };
    assert_eq!(request.factor, 1.25);
    assert_eq!(request.output, OutputId(1));
    assert!(receiver.try_recv().is_err());
}
