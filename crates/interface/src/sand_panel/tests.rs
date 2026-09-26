use bevy::prelude::*;

pub(crate) fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .init_resource::<bevy::input_focus::InputFocus>();
    app
}

pub(crate) fn connect(app: &mut App, engine: std::sync::Arc<engine::Engine>) -> cell::CellRuntime {
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    app.insert_resource(crate::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::app::CellHandle(runtime.clone()))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    runtime
}

pub(crate) async fn settle(app: &mut App, done: impl Fn(&World) -> bool) {
    for _ in 0..2000 {
        app.update();
        if done(app.world()) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let labels: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    panic!("Sand did not settle: {labels:?}");
}

#[test]
fn migrated_sand_previews_render_without_live_sands_or_input_controls() {
    use crate::sand_store::{SandKind, SandPreview};
    let mut app = app();
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let parent = world.spawn(Node::default()).id();
    for kind in [
        SandKind::Freedoom,
        SandKind::Terminal,
        SandKind::Configuration,
        SandKind::Todo,
        SandKind::Ontology,
    ] {
        crate::sand_store::entry(world, root, parent, kind, None);
    }
    let previews: Vec<_> = world
        .query_filtered::<Entity, With<SandPreview>>()
        .iter(world)
        .collect();
    assert_eq!(previews.len(), 5);
    let mut labels = Vec::new();
    for preview in previews {
        let mut pending = vec![preview];
        while let Some(entity) = pending.pop() {
            assert!(world.get::<crate::actions::ActionButton>(entity).is_none());
            assert!(world.get::<bevy::text::EditableText>(entity).is_none());
            assert!(world.get::<crate::freedoom::FreedoomSand>(entity).is_none());
            assert!(world.get::<crate::terminal::TerminalSand>(entity).is_none());
            assert!(
                world
                    .get::<crate::configuration::ConfigurationSand>(entity)
                    .is_none()
            );
            assert!(world.get::<crate::todo::TodoSand>(entity).is_none());
            assert!(world.get::<crate::ontology::OntologySand>(entity).is_none());
            if let Some(text) = world.get::<Text>(entity) {
                labels.push(text.0.as_str());
            }
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
        }
    }
    for caption in ["Start / restart", "Open shell", "Save identity", "Add task"] {
        assert!(
            labels.contains(&caption),
            "Missing preview control: {caption}"
        );
    }
}
