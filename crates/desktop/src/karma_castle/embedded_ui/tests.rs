use super::*;

#[allow(dead_code)]
mod karma {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/tests/support/karma.rs"
    ));
}

#[tokio::test]
async fn native_enrollment_access_edits_and_disable_wait_for_confirmed_responses() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    karma::authorize(&engine).await;
    let mut app = crate::sand_panel::tests::app();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .add_plugins(KarmaCastlePlugin);
    crate::sand_panel::tests::connect(&mut app, engine.clone());
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = super::super::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        KarmaCastle::default(),
    );
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<View>(owner).is_some_and(|view| view.ready)
    })
    .await;
    toggle(app.world_mut(), owner);
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Devices>(owner).unwrap().pending.is_none()
    })
    .await;
    let panel = app.world().get::<Devices>(owner).unwrap().panel;
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::Flex
    );
    Control::New.apply(app.world_mut(), owner);
    let inputs: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .filter(|(_, input)| input.owner == owner)
        .map(|(entity, input)| (entity, input.index))
        .collect();
    for (entity, index) in inputs {
        let value = match index {
            0 => "Kitchen ESP",
            1 => "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
            2 => "kitchen-temperature",
            3 => "30",
            4 => "-20",
            5 => "60",
            _ => continue,
        };
        app.world_mut()
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
    }
    Control::Save.apply(app.world_mut(), owner);
    assert!(app.world().get::<Devices>(owner).unwrap().pending.is_some());
    crate::sand_panel::tests::settle(&mut app, |world| {
        world
            .get::<Devices>(owner)
            .is_some_and(|state| state.pending.is_none() && state.rows.len() == 1)
    })
    .await;
    let row = app.world().get::<Devices>(owner).unwrap().rows[0].clone();
    assert!(!row.enabled);
    assert_eq!(row.cooldown_seconds, 30);
    let signal = store::records::get(&engine.store.pool, &row.signal_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.kind, "signal");
    assert_eq!(signal.slug.as_deref(), Some("kitchen-temperature"));
    Control::SetEnabled(row.uid.clone(), row.revision, true).apply(app.world_mut(), owner);
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Devices>(owner).unwrap().pending.is_none()
    })
    .await;
    let row = app.world().get::<Devices>(owner).unwrap().rows[0].clone();
    assert!(row.enabled);
    Control::Edit(row.uid.clone()).apply(app.world_mut(), owner);
    let cooldown = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| input.owner == owner && input.index == 3)
        .map(|(entity, _)| entity)
        .unwrap();
    app.world_mut()
        .get_mut::<EditableText>(cooldown)
        .unwrap()
        .editor
        .set_text("10");
    Control::Save.apply(app.world_mut(), owner);
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Devices>(owner).unwrap().pending.is_none()
    })
    .await;
    let row = app.world().get::<Devices>(owner).unwrap().rows[0].clone();
    assert_eq!(row.cooldown_seconds, 10);
    assert_eq!(row.credential_epoch, 1);
    assert!(row.enabled);
    Control::IssueToken(row.uid.clone(), row.revision).apply(app.world_mut(), owner);
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Devices>(owner).unwrap().pending.is_none()
    })
    .await;
    let state = app.world().get::<Devices>(owner).unwrap();
    let (endpoint, token) = state.issued_token.as_ref().unwrap();
    assert_eq!(endpoint, &row.uid);
    assert_eq!(token.len(), 64);
    let row = state.rows[0].clone();
    assert_eq!(row.credential_epoch, 2);
    Control::SetEnabled(row.uid, row.revision, false).apply(app.world_mut(), owner);
    crate::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Devices>(owner).unwrap().pending.is_none()
    })
    .await;
    assert!(!app.world().get::<Devices>(owner).unwrap().rows[0].enabled);
}
