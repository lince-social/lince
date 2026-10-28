use super::*;
use std::{path::Path, sync::Arc};

mod lifetime;

fn fixture(provider: cell::CellRuntime, directory: &Path) -> (App, Entity) {
    let (mut app, root, unused) = crate::native::protein_area::tests::fixture();
    app.world_mut().despawn(unused);
    app.add_plugins((bevy::input::InputPlugin, crate::native::actions::ActionsPlugin, crate::controls::editing::TextEditingPlugin))
        .init_resource::<bevy::text::FontCx>()
        .init_resource::<bevy::text::LayoutCx>()
        .init_resource::<bevy::clipboard::Clipboard>()
        .add_systems(PostUpdate, bevy::text::apply_text_edits.in_set(bevy::text::EditableTextSystems));
    let font = Font::from_bytes(include_bytes!("../../../../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
    app.world_mut().resource_mut::<bevy::text::FontCx>().collection.register_fonts(font.data, None);
    app.world_mut().resource_mut::<bevy::text::FontCx>().set_sans_serif_family("Lato").unwrap();
    app.insert_resource(crate::native::app::CellHandle(provider))
        .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::native::workspace::WorkspaceFile::new(directory.join("interface.json")))
        .add_plugins(crate::native::cell_bridge::CellBridgePlugin)
        .add_systems(Startup, configuration::load);
    super::super::install(&mut app);
    (app, root)
}

async fn until(app: &mut App, predicate: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.update();
        if predicate(app.world()) { return; }
        assert!(Instant::now() < deadline, "The actual Cell/editor operation did not settle");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

fn set(world: &mut World, entity: Entity, value: &str) {
    let mut input = world.get_mut::<EditableText>(entity).unwrap();
    input.pending_edits.clear();
    input.editor.set_text(value);
}

fn provider(engine: Arc<engine::Engine>) -> cell::CellRuntime {
    cell::CellRuntime { store: engine.store.clone(), engine, commands: Default::default(), lanes: Arc::new(cell::LaneHub::new()), wire: Default::default(), information: None, speech: None, fiote: None }
}

#[tokio::test]
async fn ordinary_rule_save_uses_the_real_cell_acknowledgement_and_reloads_before_startup() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let provider = provider(engine.clone());
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(provider.clone(), directory.path());
    until(&mut app, |world| world.get_resource::<Catalog>().is_some_and(|catalog| catalog.sender.is_some())).await;
    editor::open(app.world_mut(), root, None, None, Some("org.example.Editor".into()));
    let owner = app.world_mut().query_filtered::<Entity, With<Editor>>().single(app.world()).unwrap();
    let name = app.world().get::<Editor>(owner).unwrap().name.unwrap();
    set(app.world_mut(), name, "Editor windows");
    EditChange::Flag(0).apply(app.world_mut(), owner);
    EditChange::Flag(0).apply(app.world_mut(), owner);
    let width = app.world_mut().query::<(Entity, &Name)>().iter(app.world()).find(|(_, name)| name.as_str() == "Initial width (blank inherits)").unwrap().0;
    set(app.world_mut(), width, "1/2");
    app.update();
    editor::refresh(app.world_mut());
    let save = app.world().get::<Editor>(owner).unwrap().save;
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(save).is_none());
    app.world_mut().trigger(bevy::ui_widgets::Activate { entity: save });
    until(&mut app, |world| world.get::<Editor>(owner).is_some_and(|state| !state.pending && state.revision == 1)).await;
    let uid = app.world().get::<Editor>(owner).unwrap().uid.clone().unwrap();
    let stored = store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().unwrap();
    let rule: Rule = serde_json::from_value(stored).unwrap();
    assert_eq!(rule.properties.floating, Some(false));
    assert_eq!(rule.properties.width, Some(Dimension::Fraction([1, 2])));
    assert_eq!(active_order(app.world()), vec![uid.clone()]);
    EditChange::Cancel.apply(app.world_mut(), owner);
    assert!(app.world().get_entity(owner).is_err());
    drop(app);
    let (mut restarted, _) = fixture(provider, directory.path());
    until(&mut restarted, |world| active_order(world) == vec![uid.clone()] && world.get_resource::<Catalog>().is_some_and(|catalog| catalog.rules.entries.contains_key(&uid))).await;
    let decision = super::super::capture(restarted.world(), &serde_json::json!({"app_id":"org.example.Editor"}), None);
    assert_eq!(decision.matched.unwrap().uid, uid);
    assert_eq!(decision.properties.width, Some(Dimension::Fraction([1, 2])));
}

#[tokio::test]
async fn external_record_change_preserves_the_dirty_editor_until_explicit_reload() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine.act(engine::actions::Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Plain, head: "Existing owner rule".into(), body: String::new(), quantity: 0.0 }, None).await.unwrap().created.unwrap();
    let mut rule = Rule { revision: 1, enabled: true, match_all: true, conditions: vec![nucleus::property_rule::PropertyRule { property: nucleus::property_rule::Property::ApplicationId, value: "org.example.Editor".into() }], properties: Properties::default() };
    engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: serde_json::to_value(&rule).unwrap() }, None).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture(provider(engine.clone()), directory.path());
    until(&mut app, |world| world.get_resource::<Catalog>().is_some_and(|catalog| catalog.sender.is_some())).await;
    editor::open(app.world_mut(), root, Some(uid.clone()), None, None);
    let owner = app.world_mut().query_filtered::<Entity, With<Editor>>().single(app.world()).unwrap();
    until(&mut app, |world| world.get::<Editor>(owner).is_some_and(|editor| editor.loaded && !editor.reload)).await;
    EditChange::Flag(2).apply(app.world_mut(), owner);
    rule.revision = 2;
    rule.properties.floating = Some(true);
    engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: serde_json::to_value(&rule).unwrap() }, None).await.unwrap();
    until(&mut app, |world| world.get_resource::<Catalog>().and_then(|catalog| catalog.rules.entries.get(&uid)).is_some_and(|entry| entry.payload.as_ref().is_ok_and(|payload| payload.revision() == 2))).await;
    let state = app.world().get::<Editor>(owner).unwrap();
    assert_eq!(state.form.flags[2], Some(true));
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(state.save).is_some());
    let status = app.world().get::<Text>(state.status).unwrap().0.clone();
    assert!(status.contains("changed"));
    EditChange::Reload.apply(app.world_mut(), owner);
    until(&mut app, |world| world.get::<Editor>(owner).is_some_and(|editor| !editor.reloading && editor.revision == 2)).await;
    let state = app.world().get::<Editor>(owner).unwrap();
    assert_eq!(state.form.flags, [Some(true), None, None]);
    assert!(!state.reload);
}
