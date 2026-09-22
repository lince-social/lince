use super::*;
use std::{sync::Arc, time::Duration};

async fn until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            app.update();
            if condition(app.world_mut()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn slug_selection_saves_the_description_and_restores_the_castle() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let original = "Hello, world\n\nTesting123";
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("shader-test".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Shader test".into(),
                body: original.into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .init_resource::<bevy::picking::hover::HoverMap>()
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::app::CellHandle(cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            information: None,
        }))
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::protein_area::ProteinAreaPlugin,
            crate::record_binding::RecordBindingPlugin,
            crate::description::DescriptionPlugin,
            ShaderCastlePlugin,
            castle_feed::FeedPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    let owner = spawn(app.world_mut(), root, 1, DVec2::new(60.0, 70.0), "");
    let area = app.world().get::<Frame>(owner).unwrap().area;
    let slug = app.world().get::<ShaderCastle>(owner).unwrap().slug;
    app.world_mut()
        .get_mut::<EditableText>(slug)
        .unwrap()
        .editor
        .set_text("@shader-test");
    Select.apply(app.world_mut(), owner);
    until(&mut app, |world| {
        !castle_feed::cards(world, area).is_empty()
    })
    .await;
    let input = app
        .world_mut()
        .query::<(Entity, &crate::record_binding::TextBinding)>()
        .iter(app.world())
        .find(|(_, binding)| binding.record.area == area && binding.property == "body")
        .unwrap()
        .0;
    app.world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(input, bevy::input_focus::FocusCause::Navigated);
    until(&mut app, |world| {
        crate::record_binding::status(
            world,
            &crate::protein_area::RecordBinding {
                area,
                uid: uid.clone(),
                source: Source::Local,
            },
        )
        .is_some_and(|status| status == "Saved")
    })
    .await;
    let edited = format!(
        "Hello, world\n\n```wgsl\n{}```\n\nTesting123",
        crate::description::SHADER_EXAMPLE
    );
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&edited);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            app.update();
            if engine.doc_text(&uid).await.unwrap().1 == edited {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        app.world_mut()
            .query::<&crate::description::Description>()
            .iter(app.world())
            .any(|description| description.source == edited)
    );
    let saved = snapshot(app.world_mut(), root).pop().unwrap();
    assert!(saved.valid());
    let json = serde_json::to_string(&saved).unwrap();
    app.world_mut().despawn(owner);
    let saved: SavedShader = serde_json::from_str(&json).unwrap();
    saved.restore(app.world_mut(), root);
    let restored = app
        .world_mut()
        .query::<(Entity, &ShaderCastle)>()
        .iter(app.world())
        .next()
        .unwrap()
        .0;
    let frame = app.world().get::<Frame>(restored).unwrap();
    assert_eq!(
        app.world()
            .get::<crate::area::InfluenceArea>(frame.area)
            .unwrap()
            .protein
            .as_ref()
            .unwrap()
            .draft
            .query["where"][0]["all"][0]["slug_eq"],
        "shader-test"
    );
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(restored)
            .unwrap()
            .position,
        DVec2::new(60.0, 70.0)
    );
    let slug = app.world().get::<ShaderCastle>(restored).unwrap().slug;
    let area = frame.area;
    app.world_mut()
        .get_mut::<EditableText>(slug)
        .unwrap()
        .editor
        .set_text("missing-shader");
    Select.apply(app.world_mut(), restored);
    until(&mut app, |world| {
        crate::protein_area::feed_ready(world, area) && castle_feed::cards(world, area).is_empty()
    })
    .await;
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, edited);
}
