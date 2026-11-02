use super::*;
use crate::native::{sand_panel, shortcuts::{Settings, configuration::{self, LayoutSettings}}};

#[derive(Component)]
struct GapEditor {
    field: Entity,
    status: Entity,
}

#[derive(Clone)]
struct Save;

impl Action for Save {
    fn apply(&self, world: &mut World, page: Entity) {
        let Some(editor) = world.get::<GapEditor>(page) else { return; };
        let (field, status) = (editor.field, editor.status);
        let Some(input) = world.get::<EditableText>(field) else { return; };
        if input.is_composing() || input.pending_paste.is_some() { return; }
        let result = input.value().to_string().trim().parse::<u16>().ok().filter(|value| *value <= 256)
            .ok_or_else(|| "Use a whole gap from 0 to 256 logical pixels.".to_owned())
            .and_then(|gap| {
                let mut settings = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
                settings.gaps = gap;
                let bindings = world.get_resource::<Settings>().cloned().unwrap_or_default();
                configuration::commit(world, bindings, settings)
            });
        if let Some(mut text) = world.get_mut::<Text>(status) { text.0 = result.unwrap_or_else(|error| error); }
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.ring(); }
    }
}

pub(super) fn panel(world: &mut World, parent: Entity) {
    let value = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default().gaps;
    let page = world.spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() },
        Name::new("customization.window-gap"), ChildOf(parent))).id();
    let field = sand_panel::field(world, page, "Tiling window gap (0–256 logical pixels)", &value.to_string());
    if let Some(slider) = crate::native::slider::spawn(world, page, "Tiling window gap", crate::native::slider::SliderSand {
        start: 0.0, end: 256.0, step: 1.0, decimals: 0,
    }, f32::from(value), " px") {
        world.entity_mut(slider).observe(move |event: On<crate::native::slider::SliderChanged>, mut commands: Commands| {
            let value = format!("{:.0}", event.value);
            commands.queue(move |world: &mut World| {
                if let Some(mut input) = world.get_mut::<EditableText>(field) { input.editor.set_text(&value); }
            });
        });
    }
    label(world, page, "Zero places tiled windows edge to edge. Save applies the gap to every workspace and keeps it after restart.", 13.0);
    let status = label(world, page, "", 13.0);
    sand_panel::button(world, page, page, "Save tiling window gap", Save);
    world.entity_mut(page).insert(GapEditor { field, status });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{container::BoxRoot, edit_mode::{EditAction, EditModePlugin}, workspace::{WorkspaceFile, WorkspacePlugin}};

    fn interface(path: &std::path::Path) -> (App, Entity) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .insert_resource(WorkspaceFile::new(path.join("interface.json")))
            .add_plugins((WorkspacePlugin, EditModePlugin));
        let root = app.world_mut().spawn(BoxRoot).id();
        app.update();
        (app, root)
    }

    #[test]
    fn customization_gap_accepts_typing_and_sliding_and_persists_without_changing_shortcuts() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, root) = interface(directory.path());
        let original = app.world().resource::<Settings>().clone();
        assert_eq!(app.world().resource::<LayoutSettings>().gaps, 0);
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Customization.apply(app.world_mut(), root);
        app.update();
        let page = app.world_mut().query_filtered::<Entity, With<GapEditor>>().single(app.world()).unwrap();
        let field = app.world().get::<GapEditor>(page).unwrap().field;
        app.world_mut().get_mut::<EditableText>(field).unwrap().editor.set_text("24");
        CustomizationAction::Scheme(ColorScheme::Light).apply(app.world_mut(), root);
        assert_eq!(app.world().get::<EditableText>(field).unwrap().value().to_string(), "24");
        Save.apply(app.world_mut(), page);
        assert_eq!(app.world().resource::<LayoutSettings>().gaps, 24);
        assert_eq!(app.world().resource::<Settings>(), &original);
        app.world_mut().get_mut::<EditableText>(field).unwrap().editor.set_text("257");
        Save.apply(app.world_mut(), page);
        assert_eq!(app.world().resource::<LayoutSettings>().gaps, 24);
        let status = app.world().get::<GapEditor>(page).unwrap().status;
        assert!(app.world().get::<Text>(status).unwrap().0.contains("0 to 256"));
        let slider = app.world_mut().query::<(Entity, &AccessibilityNode)>().iter(app.world())
            .find(|(entity, node)| node.label() == Some("Tiling window gap") && app.world().get::<crate::native::slider::SliderSand>(*entity).is_some()).unwrap().0;
        app.world_mut().trigger(crate::native::slider::SliderChanged { entity: slider, value: 0.0 });
        app.world_mut().flush();
        Save.apply(app.world_mut(), page);
        assert_eq!(app.world().resource::<LayoutSettings>().gaps, 0);
        let (restarted, _) = interface(directory.path());
        assert_eq!(restarted.world().resource::<LayoutSettings>().gaps, 0);
        assert_eq!(restarted.world().resource::<Settings>(), &original);
    }
}
