use super::*;
use std::sync::Arc;

async fn step(world: &mut World) -> (u64, usize) {
    let mut messages = Vec::new();
    {
        let mut bridge = world.non_send_mut::<crate::native::cell_bridge::CellBridge>();
        while let Ok(message) = bridge.incoming.try_recv() {
            messages.push(message);
        }
    }
    let count = messages.len();
    for message in messages {
        crate::native::description::receive_live(world, &Source::Local, &message);
        receive(world, Source::Local, message);
    }
    let started = Instant::now();
    update(world);
    for document in world.resource::<Bindings>().documents.values() {
        assert!(!document.failed, "{}", document.status);
    }
    (started.elapsed().as_micros() as u64, count)
}

async fn settle(world: &mut World) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        step(world).await;
        if !world.resource::<Bindings>().documents.is_empty()
            && world
                .resource::<Bindings>()
                .documents
                .values()
                .all(|document| {
                    document.loaded
                        && document.joined
                        && document.pending.is_empty()
                        && !document.saving
                        && document.saved == document.revision
                })
        {
            return;
        }
        assert!(Instant::now() < deadline, "Private edit did not settle");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

fn body(bytes: usize) -> String {
    let mut state = 0x517cc1b727220a95_u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(b'a' + (state % 26) as u8)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private file-backed edit phase measurement"]
async fn file_backed_local_edit_phase_probe() {
    let bytes: usize = std::env::var("LINCE_EDIT_PROBE_BYTES")
        .unwrap_or_else(|_| "1024".into())
        .parse()
        .unwrap();
    let views: usize = std::env::var("LINCE_EDIT_PROBE_VIEWS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let field = std::env::var("LINCE_EDIT_PROBE_FIELD").unwrap_or_else(|_| "body".into());
    let position = std::env::var("LINCE_EDIT_PROBE_POSITION").unwrap_or_else(|_| "end".into());
    assert!([0, 1024, 65536, 1048576].contains(&bytes));
    assert!([1, 8].contains(&views));
    assert!(["head", "body"].contains(&field.as_str()));
    assert!(["start", "end"].contains(&position.as_str()));
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("private-edit.sqlite");
    let url = format!("sqlite://{}", database.display());
    let engine = Arc::new(engine::Engine::open(&url).await.unwrap());
    let mut expected_head = if bytes == 0 {
        String::new()
    } else {
        "Local synthetic title".to_string()
    };
    let body_bytes = bytes.min(engine::collab::limits().text_bytes - expected_head.len() - 3);
    let mut expected_body = body(body_bytes);
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: expected_head.clone(),
                body: if bytes > 262144 {
                    String::new()
                } else {
                    expected_body.clone()
                },
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    if bytes > 262144 {
        engine
            .write_record_text(&uid, None, Some(&expected_body))
            .await
            .unwrap();
    }
    let journal: String = store::sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let synchronous: i64 = store::sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(synchronous, 2);
    let runtime = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut world = World::new();
    world.insert_resource(crate::native::app::CellHandle(runtime.clone()));
    world.init_resource::<Bindings>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    world.insert_non_send(crate::native::cell_bridge::connect(
        runtime,
        crate::native::wake::WakeSignal::new(|| {}),
    ));
    let value = if field == "head" {
        &expected_head
    } else {
        &expected_body
    };
    let mut editors = Vec::new();
    for _ in 0..views {
        let entity = world.spawn(EditableText::new(value)).id();
        attach(
            &mut world,
            entity,
            RecordBinding {
                area: entity,
                uid: uid.clone(),
                source: Source::Local,
            },
            &field,
            None,
        );
        editors.push(entity);
    }
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(editors[0], bevy::input_focus::FocusCause::Pressed);
    settle(&mut world).await;
    let mut idle_us = Vec::new();
    for _ in 0..5 {
        idle_us.push(step(&mut world).await.0);
    }
    performance::flush();
    let mut rounds = Vec::new();
    for suffix in ['x', 'y', 'z'] {
        let value = if field == "head" {
            &mut expected_head
        } else {
            &mut expected_body
        };
        if position == "start" {
            value.insert(0, suffix);
        } else {
            value.push(suffix);
        }
        let started = Instant::now();
        world
            .get_mut::<EditableText>(editors[0])
            .unwrap()
            .editor
            .set_text(value);
        let set_text_us = started.elapsed().as_micros() as u64;
        let first_update_us = step(&mut world).await.0;
        for entity in &editors {
            assert_eq!(
                world
                    .get::<EditableText>(*entity)
                    .unwrap()
                    .value()
                    .to_string(),
                *value
            );
        }
        let mirror_observed_us = started.elapsed().as_micros() as u64;
        let revision = world
            .resource::<Bindings>()
            .documents
            .values()
            .next()
            .unwrap()
            .revision;
        let mut durable_observed_us = None;
        let mut acknowledged_observed_us = None;
        let mut updates_us = vec![first_update_us];
        let mut messages = 0;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let (elapsed, count) = step(&mut world).await;
            updates_us.push(elapsed);
            messages += count;
            let document = world
                .resource::<Bindings>()
                .documents
                .values()
                .next()
                .unwrap();
            if document.saved >= revision {
                durable_observed_us.get_or_insert(started.elapsed().as_micros() as u64);
            }
            if document.pending.is_empty() {
                acknowledged_observed_us.get_or_insert(started.elapsed().as_micros() as u64);
            }
            if document.pending.is_empty()
                && !document.saving
                && document.saved == document.revision
            {
                break;
            }
            assert!(updates_us.len() < 1000 && Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let settled_observed_us = started.elapsed().as_micros() as u64;
        assert_eq!(
            engine.doc_text(&uid).await.unwrap(),
            (expected_head.clone(), expected_body.clone())
        );
        rounds.push(serde_json::json!({
            "revision": revision,
            "set_text_us": set_text_us,
            "mirror_observed_us": mirror_observed_us,
            "durable_recovery_observed_us": durable_observed_us.unwrap(),
            "acknowledged_observed_us": acknowledged_observed_us.unwrap(),
            "settled_observed_us": settled_observed_us,
            "update_us": updates_us,
            "messages": messages,
        }));
    }
    performance::flush();
    let storage = storage_source(
        &(serde_json::to_string(&Source::Local).unwrap(), uid.clone()),
        None,
    );
    assert!(
        engine
            .load_record_edit_draft(&storage, &uid)
            .await
            .unwrap()
            .is_none()
    );
    drop(world);
    tokio::time::timeout(Duration::from_secs(10), engine.store.pool.close())
        .await
        .unwrap();
    drop(engine);
    let reopened = engine::Engine::open(&url).await.unwrap();
    assert_eq!(
        reopened.doc_text(&uid).await.unwrap(),
        (expected_head, expected_body)
    );
    reopened.store.pool.close().await;
    let report = serde_json::json!({
        "case": "file-backed-local-edit-phases",
        "field": field,
        "position": position,
        "requested_body_bytes": bytes,
        "body_bytes": body_bytes,
        "views": views,
        "journal": journal,
        "synchronous": synchronous,
        "debug_assertions": cfg!(debug_assertions),
        "idle_update_us": idle_us,
        "rounds": rounds,
        "reopened_exactly": true,
        "timing_scope": "headless native binding and real local Cell/engine/file-backed SQL; callback observations include bounded polling and full updates; physical presentation, per-statement SQL, fsync and engine subphases unobserved",
    });
    if let Ok(path) = std::env::var("LINCE_EDIT_PROBE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    eprintln!("Local edit phase probe {report}");
}

#[test]
fn current_unicode_selection_survives_stale_binding_text_and_remote_insertion() {
    let value = "éA👩‍💻עברית";
    let document = Document::new(Source::Local, None);
    document.doc.get_text("body").insert(0, value).unwrap();
    document.doc.commit();
    let binding = TextBinding {
        record: RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: nucleus::new_uid("r"),
            source: Source::Local,
        },
        property: "body".into(),
        status: None,
        observed: "Old text".into(),
        confirmed: "Old text".into(),
    };
    let mut fonts = FontCx::default();
    let font = Font::from_bytes(
        include_bytes!("../../../../../institute/assets/fonts/DejaVuSans/DejaVuSansMono.ttf")
            .to_vec(),
    );
    fonts.collection.register_fonts(font.data, None);
    fonts.set_sans_serif_family("DejaVu Sans Mono").unwrap();
    let mut layout = LayoutCx::default();
    let mut text = EditableText::new(value);
    for (anchor, focus) in [(0, 0), (2, 2), (0, value.len()), (value.len(), 2)] {
        text.editor
            .driver(&mut fonts.context, &mut layout.0)
            .select_byte_range(anchor, focus);
        let actual = text.editor.raw_selection();
        let positions = (
            value[..actual.anchor().index()].chars().count(),
            value[..actual.focus().index()].chars().count(),
        );
        let selected = selection(&document, &binding, &text);
        assert_eq!(
            document
                .doc
                .get_cursor_pos(selected.0.as_ref().unwrap())
                .unwrap()
                .current
                .pos,
            positions.0
        );
        assert_eq!(
            document
                .doc
                .get_cursor_pos(selected.1.as_ref().unwrap())
                .unwrap()
                .current
                .pos,
            positions.1
        );
    }
    text.editor
        .driver(&mut fonts.context, &mut layout.0)
        .select_byte_range(2, value.len());
    let selected = selection(&document, &binding, &text);
    document.doc.get_text("body").insert(0, "新").unwrap();
    document.doc.commit();
    assert_eq!(
        document
            .doc
            .get_cursor_pos(selected.0.as_ref().unwrap())
            .unwrap()
            .current
            .pos,
        2
    );
    assert_eq!(
        document
            .doc
            .get_cursor_pos(selected.1.as_ref().unwrap())
            .unwrap()
            .current
            .pos,
        value.chars().count() + 1
    );
}
