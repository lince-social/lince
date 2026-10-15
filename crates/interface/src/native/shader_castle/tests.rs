use super::*;
use std::{sync::Arc, time::Duration};

fn press(world: &mut World, owner: Entity, caption: &str) {
    let button = world
        .query::<(
            &crate::native::actions::ActionButton,
            &crate::native::icons::Tooltip,
        )>()
        .iter(world)
        .find(|(button, tooltip)| button.target == owner && tooltip.0 == caption)
        .unwrap()
        .0
        .clone();
    button.actions.run(world, button.target);
}

fn editor_draft(world: &mut World, owner: Entity) -> serde_json::Value {
    let editor = world.get::<ShaderCastle>(owner).unwrap().editor.unwrap();
    serde_json::to_value(shader_effects::capture_editor(world, editor).unwrap()).unwrap()
}

async fn until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            crate::native::description::shader::prepare_test_previews(app.world_mut());
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
async fn typed_extension_creation_preserves_description_and_restores_the_castle() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::ensure_local(&engine.store.pool, "http://shader-castle")
        .await
        .unwrap()
        .uid;
    engine
        .set_signer(engine::trust::Signer::generate(&organ, "key"))
        .await
        .unwrap();
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
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<crate::native::tokens::ThemeSettings>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .init_resource::<bevy::picking::hover::HoverMap>()
        .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            speech: None,
            information: None,
        }))
        .add_plugins((
            crate::native::cell_bridge::CellBridgePlugin,
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::record_binding::RecordBindingPlugin,
            crate::native::description::DescriptionPlugin,
            ShaderCastlePlugin,
            shader_effects::ShaderEffectsPlugin,
            castle_feed::FeedPlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::native::workspace::Workspaces::default(),
            crate::native::canvas::CanvasView::default(),
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
    until(&mut app, |world| {
        world.get::<ShaderCastle>(owner).unwrap().editor.is_some()
    })
    .await;
    AddBall.apply(app.world_mut(), owner);
    until(&mut app, |world| example_visible(world, owner)).await;
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, original);
    let editor = app
        .world()
        .get::<ShaderCastle>(owner)
        .unwrap()
        .editor
        .unwrap();
    assert!(
        app.world()
            .get::<crate::native::canvas::CanvasItem>(editor)
            .is_none()
    );
    assert!(shader_effects::snapshot(app.world_mut(), root).is_empty());
    engine
        .act(
            engine::actions::Action::SetQuantity {
                target: uid.clone(),
                value: 3.0,
            },
            None,
        )
        .await
        .unwrap();
    until(&mut app, |world| {
        editor_draft(world, owner)["quantity"] == "3"
    })
    .await;
    let quantity = app
        .world_mut()
        .query::<(Entity, &EditableText)>()
        .iter(app.world())
        .find(|(_, text)| text.value() == "3")
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(quantity)
        .unwrap()
        .editor
        .set_text("12.5");
    let draft = shader_effects::capture_editor(app.world_mut(), editor).unwrap();
    let draft = serde_json::to_value(draft).unwrap();
    assert_eq!(draft["record"], uid);
    assert!(draft["revision"].as_u64().unwrap() > 0);
    assert_eq!(draft["quantity"], "12.5");
    assert_eq!(draft["custom_quantity"], true);
    let source = app
        .world_mut()
        .query::<(Entity, &EditableText)>()
        .iter(app.world())
        .find(|(_, text)| text.value() == nucleus::shader_effect::ENERGY)
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(source)
        .unwrap()
        .editor
        .set_text("fn shade(");
    let saved = snapshot(app.world_mut(), root).pop().unwrap();
    assert!(saved.valid());
    assert_eq!(saved.drafts.len(), 1);
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
            .get::<crate::native::area::InfluenceArea>(frame.area)
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
            .get::<crate::native::canvas::CanvasItem>(restored)
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
        crate::native::protein_area::feed_ready(world, area)
            && castle_feed::cards(world, area).is_empty()
    })
    .await;
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, original);
    app.world_mut()
        .get_mut::<EditableText>(slug)
        .unwrap()
        .editor
        .set_text("shader-test");
    Select.apply(app.world_mut(), restored);
    until(&mut app, |world| {
        world
            .get::<ShaderCastle>(restored)
            .unwrap()
            .editor
            .is_some()
    })
    .await;
    for _ in 0..5 {
        app.update();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    until(&mut app, |world| {
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Draft restored")
    })
    .await;
    let editor = app
        .world()
        .get::<ShaderCastle>(restored)
        .unwrap()
        .editor
        .unwrap();
    let draft = shader_effects::capture_editor(app.world_mut(), editor).unwrap();
    let draft = serde_json::to_value(draft).unwrap();
    assert!(
        draft["effect"]["programs"]
            .as_object()
            .unwrap()
            .values()
            .any(|source| source == "fn shade(")
    );
    assert!(
        draft["original"]["programs"]
            .as_object()
            .unwrap()
            .values()
            .any(|source| source == nucleus::shader_effect::ENERGY)
    );
    let second = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("shader-second".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Second shader".into(),
                body: "Second description".into(),
                quantity: 2.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    app.world_mut()
        .get_mut::<EditableText>(slug)
        .unwrap()
        .editor
        .set_text("shader-second");
    Select.apply(app.world_mut(), restored);
    until(&mut app, |world| {
        world
            .get::<ShaderCastle>(restored)
            .unwrap()
            .selection
            .as_ref()
            .is_some_and(|binding| binding.uid == second)
    })
    .await;
    let source = app
        .world_mut()
        .query::<(Entity, &EditableText)>()
        .iter(app.world())
        .find(|(_, text)| text.value().to_string().starts_with("fn shade"))
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(source)
        .unwrap()
        .editor
        .set_text("fn second(");
    app.world_mut()
        .get_mut::<EditableText>(slug)
        .unwrap()
        .editor
        .set_text("shader-test");
    Select.apply(app.world_mut(), restored);
    until(&mut app, |world| {
        world
            .get::<ShaderCastle>(restored)
            .unwrap()
            .selection
            .as_ref()
            .is_some_and(|binding| binding.uid == uid)
    })
    .await;
    let saved = snapshot(app.world_mut(), root).pop().unwrap();
    assert_eq!(saved.drafts.len(), 2);
    for (uid, expected) in [(&uid, "fn shade("), (&second, "fn second(")] {
        let draft = saved
            .drafts
            .iter()
            .find(|draft| {
                draft.matches(&RecordBinding {
                    area,
                    uid: uid.clone(),
                    source: Source::Local,
                })
            })
            .unwrap();
        let draft = serde_json::to_value(draft).unwrap();
        assert!(
            draft["effect"]["programs"]
                .as_object()
                .unwrap()
                .values()
                .any(|source| source == expected)
        );
    }
    let editor = app
        .world()
        .get::<ShaderCastle>(restored)
        .unwrap()
        .editor
        .unwrap();
    until(&mut app, |world| {
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Draft restored")
    })
    .await;
    press(app.world_mut(), editor, "Reload working");
    until(&mut app, |world| example_visible(world, restored)).await;
    press(app.world_mut(), editor, "Reset");
    until(&mut app, |world| example_visible(world, restored)).await;
    assert_eq!(
        editor_draft(app.world_mut(), restored)["effect"],
        serde_json::to_value(nucleus::shader_effect::Effect::default()).unwrap()
    );
    press(app.world_mut(), editor, "Disable / Enable");
    assert!(
        app.world_mut()
            .query::<&crate::native::description::shader::EffectPreview>()
            .iter(app.world())
            .any(|preview| preview.priority && preview.disabled)
    );
    press(app.world_mut(), editor, "Disable / Enable");
    let remote = nucleus::shader_effect::Extension::single(
        2,
        nucleus::shader_effect::Effect {
            programs: [("remote".into(), nucleus::shader_effect::GRADIENT.into())].into(),
            rules: vec![],
            fallback: "remote".into(),
            ..default()
        },
    );
    engine
        .act(
            engine::actions::Action::SetExtension {
                target: uid.clone(),
                namespace: nucleus::shader_effect::NAMESPACE.into(),
                fds: serde_json::to_value(&remote).unwrap(),
            },
            None,
        )
        .await
        .unwrap();
    press(app.world_mut(), editor, "Save / Apply");
    until(&mut app, |world| {
        example_visible(world, restored)
            && world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains("Reload") && text.0.contains("changed"))
    })
    .await;
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &uid, nucleus::shader_effect::NAMESPACE)
            .await
            .unwrap()
            .unwrap(),
        serde_json::to_value(&remote).unwrap()
    );
    press(app.world_mut(), editor, "Reload working");
    until(&mut app, |world| {
        example_visible(world, restored) && editor_draft(world, restored)["revision"] == 2
    })
    .await;
    assert_eq!(
        editor_draft(app.world_mut(), restored)["effect"],
        serde_json::to_value(remote.selected(None).unwrap()).unwrap()
    );
    prepare_source(
        app.world_mut(),
        restored,
        Source::Organ(nucleus::new_uid("r")),
    );
    until(&mut app, |world| {
        world
            .get::<ShaderCastle>(restored)
            .unwrap()
            .editor
            .is_none()
    })
    .await;
    let remote_source = nucleus::new_uid("r");
    let runtime = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .clone();
    app.world_mut()
        .init_resource::<crate::native::practice_cells::PracticeCells>();
    app.world_mut()
        .resource_mut::<crate::native::practice_cells::PracticeCells>()
        .cells
        .insert(remote_source.clone(), runtime);
    prepare_source(
        app.world_mut(),
        restored,
        Source::Organ(remote_source.clone()),
    );
    until(&mut app, |world| example_visible(world, restored)).await;
    let remote_draft = editor_draft(app.world_mut(), restored);
    assert_eq!(
        remote_draft["source"],
        serde_json::to_value(Source::Organ(remote_source)).unwrap()
    );
    assert_eq!(remote_draft["quantity"], "3");
    assert_eq!(remote_draft["custom_quantity"], false);
    prepare_source(app.world_mut(), restored, Source::Local);
    until(&mut app, |world| example_visible(world, restored)).await;
    assert_eq!(
        editor_draft(app.world_mut(), restored)["source"],
        serde_json::to_value(Source::Local).unwrap()
    );
    assert_eq!(editor_draft(app.world_mut(), restored)["revision"], 2);
    assert_eq!(editor_draft(app.world_mut(), restored)["quantity"], "12.5");
    let editor = app
        .world()
        .get::<ShaderCastle>(restored)
        .unwrap()
        .editor
        .unwrap();
    press(app.world_mut(), editor, "Follow Record quantity");
    until(&mut app, |world| {
        let draft = editor_draft(world, restored);
        draft["quantity"] == "3" && draft["custom_quantity"] == false
    })
    .await;
    assert_eq!(engine.doc_text(&uid).await.unwrap().1, original);
}
