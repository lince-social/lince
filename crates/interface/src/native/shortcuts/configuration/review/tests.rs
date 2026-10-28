use super::*;
use crate::layout::desktop::OverviewSettings;

#[derive(Component)]
struct Draft {
    base: OverviewSettings,
    local: OverviewSettings,
    applied: usize,
}

fn snapshot(world: &World, owner: Entity) -> Result<Snapshot, String> {
    let draft = world.get::<Draft>(owner).ok_or("The original editor is closed.")?;
    Snapshot::new(&draft.base, &draft.local, &world.resource::<LayoutSettings>().overview)
}

fn apply(world: &mut World, owner: Entity, active: Value, value: Value) -> Result<(), String> {
    let active: OverviewSettings = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let value: OverviewSettings = serde_json::from_value(value).map_err(|error| error.to_string())?;
    value.validate()?;
    let mut draft = world.get_mut::<Draft>(owner).ok_or("The original editor is closed.")?;
    draft.base = active;
    draft.local = value;
    draft.applied += 1;
    Ok(())
}

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((bevy::input::InputPlugin, crate::native::actions::ActionsPlugin))
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<InputFocus>()
        .insert_resource(LayoutSettings { overview: OverviewSettings { hover_delay_ms: 800, edge_scroll_percent: 25 }, ..Default::default() });
    install(&mut app);
    let root = app.world_mut().spawn((Node::default(), InheritedVisibility::VISIBLE, crate::native::workspace::Workspaces::default())).id();
    let owner = app.world_mut().spawn((Node::default(), InheritedVisibility::VISIBLE, Draft { base: OverviewSettings { hover_delay_ms: 400, edge_scroll_percent: 0 }, local: OverviewSettings { hover_delay_ms: 650, edge_scroll_percent: 0 }, applied: 0 }, ChildOf(root))).id();
    let previous = app.world_mut().spawn((Node::default(), InheritedVisibility::VISIBLE, ChildOf(owner))).id();
    app.world_mut().resource_mut::<InputFocus>().set(previous, FocusCause::Navigated);
    shortcuts::own_keyboard(app.world_mut(), owner);
    open(app.world_mut(), owner, "fixture settings", snapshot, apply).unwrap();
    app.update();
    (app, owner, previous)
}

fn named(app: &mut App, name: &str) -> Entity {
    app.world_mut().query::<(Entity, &Name)>().iter(app.world()).find(|(_, candidate)| candidate.as_str() == name).unwrap().0
}

fn activate(app: &mut App, name: &str) {
    let entity = named(app, name);
    app.world_mut().trigger(bevy::ui_widgets::Activate { entity });
    app.update();
}

#[test]
fn cancelling_review_preserves_the_editor_draft_active_values_and_parent_keyboard_owner() {
    let (mut app, owner, previous) = fixture();
    activate(&mut app, "configuration.review.choice.0.local");
    activate(&mut app, "configuration.review.cancel");
    let draft = app.world().get::<Draft>(owner).unwrap();
    assert_eq!(draft.base.hover_delay_ms, 400);
    assert_eq!(draft.local.hover_delay_ms, 650);
    assert_eq!(draft.applied, 0);
    assert_eq!(app.world().resource::<LayoutSettings>().overview.hover_delay_ms, 800);
    assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, Some(owner));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(previous));
}

#[test]
fn explicit_conflict_choice_updates_only_the_original_draft_and_retains_independent_file_changes() {
    let (mut app, owner, _) = fixture();
    let confirm = named(&mut app, "configuration.review.confirm");
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_some());
    activate(&mut app, "configuration.review.choice.0.local");
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_none());
    activate(&mut app, "configuration.review.confirm");
    let draft = app.world().get::<Draft>(owner).unwrap();
    assert_eq!(draft.base, OverviewSettings { hover_delay_ms: 800, edge_scroll_percent: 25 });
    assert_eq!(draft.local, OverviewSettings { hover_delay_ms: 650, edge_scroll_percent: 25 });
    assert_eq!(draft.applied, 1);
    assert_eq!(app.world().resource::<LayoutSettings>().overview, draft.base);
    assert_eq!(app.world_mut().query::<&Session>().iter(app.world()).count(), 0);
}

#[test]
fn changed_file_and_old_queued_choice_require_a_new_comparison() {
    let (mut app, owner, _) = fixture();
    let overlay = app.world_mut().query_filtered::<Entity, With<Session>>().single(app.world()).unwrap();
    let old_sequence = app.world().get::<Session>(overlay).unwrap().sequence;
    activate(&mut app, "configuration.review.choice.0.local");
    app.world_mut().resource_mut::<LayoutSettings>().overview.hover_delay_ms = 900;
    app.update();
    let confirm = named(&mut app, "configuration.review.confirm");
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(confirm).is_some());
    activate(&mut app, "configuration.review.update");
    Change::Choose(old_sequence, 0, Choice::File).apply(app.world_mut(), overlay);
    assert!(!app.world().get::<Session>(overlay).unwrap().proposal.resolved());
    activate(&mut app, "configuration.review.choice.0.local");
    activate(&mut app, "configuration.review.confirm");
    assert_eq!(app.world().get::<Draft>(owner).unwrap().local.hover_delay_ms, 650);
    assert_eq!(app.world().get::<Draft>(owner).unwrap().base.hover_delay_ms, 900);
    assert_eq!(app.world().resource::<LayoutSettings>().overview.hover_delay_ms, 900);
}

#[test]
fn a_closed_editor_cannot_be_replaced_by_another_owner() {
    let (mut app, owner, _) = fixture();
    app.world_mut().despawn(owner);
    app.update();
    assert_eq!(app.world_mut().query::<&Session>().iter(app.world()).count(), 0);
    assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, None);
    assert_eq!(app.world().resource::<InputFocus>().get(), None);
}

#[test]
fn a_hidden_or_disabled_prior_field_is_not_given_focus_after_cancel() {
    for disabled in [false, true] {
        let (mut app, owner, previous) = fixture();
        if disabled { app.world_mut().entity_mut(previous).insert(bevy::ui::InteractionDisabled); }
        else { app.world_mut().get_mut::<Node>(previous).unwrap().display = Display::None; }
        activate(&mut app, "configuration.review.cancel");
        assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, Some(owner));
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        assert_eq!(app.world().get::<Draft>(owner).unwrap().applied, 0);
    }
}

#[test]
fn hiding_the_parent_or_changing_the_canvas_or_workspace_retires_review_without_applying() {
    for change in 0..3 {
        let (mut app, owner, _) = fixture();
        let root = app.world().get::<ChildOf>(owner).unwrap().parent();
        match change {
            0 => app.world_mut().get_mut::<Node>(owner).unwrap().display = Display::None,
            1 => app.world_mut().get_mut::<crate::native::workspace::Workspaces>(root).unwrap().canvas_id = "another-canvas".into(),
            _ => app.world_mut().get_mut::<crate::native::workspace::Workspaces>(root).unwrap().active = 2,
        }
        app.update();
        assert_eq!(app.world_mut().query::<&Session>().iter(app.world()).count(), 0);
        assert_eq!(app.world().get::<Draft>(owner).unwrap().applied, 0);
        assert_eq!(app.world().get::<Draft>(owner).unwrap().local.hover_delay_ms, 650);
        assert_eq!(app.world().resource::<LayoutSettings>().overview.hover_delay_ms, 800);
        assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, None);
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
    }
}

#[test]
fn a_replacement_modal_retains_its_keyboard_and_focused_field() {
    let (mut app, original, _) = fixture();
    let replacement = app.world_mut().spawn(Node::default()).id();
    let field = app.world_mut().spawn((Node::default(), ChildOf(replacement))).id();
    shortcuts::own_keyboard(app.world_mut(), replacement);
    app.world_mut().resource_mut::<InputFocus>().set(field, FocusCause::Navigated);
    app.update();
    assert_eq!(app.world_mut().query::<&Session>().iter(app.world()).count(), 0);
    assert_eq!(app.world().get::<Draft>(original).unwrap().applied, 0);
    assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, Some(replacement));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
}

#[test]
fn removing_the_captured_output_cannot_restore_its_original_editor_focus() {
    use crate::{host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping}, native::output_views::OutputViews};
    let (mut app, owner, _) = fixture();
    activate(&mut app, "configuration.review.cancel");
    let root = app.world().get::<ChildOf>(owner).unwrap().parent();
    let bounds = Rectangle::new([0.0; 2], [1000.0, 720.0]).unwrap();
    app.world_mut().insert_resource(OutputViews { canvas: Some(root), focused: OutputId(1), outputs: vec![OutputViewport { rotation: Default::default(),
        id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1000, 720],
        raster: RasterMapping { origin: [0.0; 2], scale: [1.0; 2] }, workspace: 1,
    }] });
    open(app.world_mut(), owner, "fixture settings", snapshot, apply).unwrap();
    app.world_mut().remove_resource::<OutputViews>();
    app.update();
    assert_eq!(app.world_mut().query::<&Session>().iter(app.world()).count(), 0);
    assert_eq!(app.world().get::<Draft>(owner).unwrap().applied, 0);
    assert_eq!(app.world().resource::<shortcuts::KeyboardModalOwner>().0, None);
    assert_eq!(app.world().resource::<InputFocus>().get(), None);
}
