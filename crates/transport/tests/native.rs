use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{Engine, actions::Action};
use fiote::tools::Registry;
use loro::LoroDoc;
use serde_json::{Value, json};
use transport::{ClientMessage, LaneHub, ServerMessage, Session, native::Context};

async fn fixture() -> (Arc<Engine>, Registry, String, String) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let agent = engine
        .act(
            Action::CreateAgent {
                head: "Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: Some("shared-note".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Shared 👩‍💻".into(),
                body: "Hello world.\nKeep this paragraph.".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut tools = Registry::default();
    Session::local(engine.clone(), Arc::new(LaneHub::new()), "fiote-test")
        .into_native_tools(Context {
            agent: agent.clone(),
            record: record.clone(),
            thread: record.clone(),
        })
        .register(&mut tools);
    (engine, tools, record, agent)
}

async fn call(tools: &Registry, name: &str, mut args: Value) -> Value {
    let encoded = match name {
        "lince_query" => Some("protein"),
        "lince_action" => Some("action"),
        _ => None,
    };
    if let Some(key) = encoded {
        if args[key].is_object() {
            args[key] = json!(args[key].to_string());
        }
    }
    let result = tools.run(name, args).await;
    assert_eq!(result["ok"], true, "{result}");
    result["result"].clone()
}

async fn read(tools: &Registry, uid: &str) -> Value {
    call(tools, "lince_read_record", json!({"record_uid":uid})).await
}

#[tokio::test]
async fn native_tools_discover_the_real_schemas_and_query_saved_proteins() {
    let (engine, tools, uid, _) = fixture().await;
    let overview = call(&tools, "lince_describe", json!({})).await;
    assert!(overview["actions"].as_array().unwrap().len() > 150);
    assert!(overview["protein_schema"]["properties"]["source"].is_object());
    for name in [
        "create-record",
        "change-record",
        "apply-area-transition",
        "create-karma-program",
    ] {
        let result = call(&tools, "lince_describe", json!({"action":name})).await;
        assert_eq!(result["schema"]["properties"]["action"]["const"], name);
    }
    let schema = call(
        &tools,
        "lince_describe",
        json!({"action":"create-karma-grant"}),
    )
    .await;
    assert_eq!(
        schema["schema"]["$defs"]["DelegationGrantSpec"]["properties"]["expires_at"]["type"],
        "string"
    );
    assert!(schema["schema"]["$defs"].as_object().unwrap().values().any(
        |definition| definition["properties"]["value"]["type"] == "string"
            && definition["properties"]["scale"]["type"] == "integer"
    ));
    engine
        .act(
            Action::SaveProtein {
                slug: "my-notes".into(),
                head: "My notes".into(),
                ast: json!({"source":"record","where":[{"all":[{"uid_eq":uid}]}],"limit":1}),
            },
            None,
        )
        .await
        .unwrap();
    let result = call(&tools, "lince_query", json!({"saved":"my-notes"})).await;
    assert_eq!(result["rows"][0]["uid"], uid);
    assert_eq!(result["rows"].as_array().unwrap().len(), 1);
    let status = call(&tools, "lince_sync_status", json!({})).await;
    assert_eq!(status["type"], "sync_status");
}

#[tokio::test]
async fn human_and_agent_edits_merge_in_both_orders_and_notify_the_live_session() {
    for human_first in [true, false] {
        let (engine, tools, uid, agent) = fixture().await;
        let mut user = Session::local(engine.clone(), Arc::new(LaneHub::new()), "human");
        let state = user
            .handle(ClientMessage::CollabJoin {
                id: "join".into(),
                record_uid: uid.clone(),
            })
            .await;
        let ServerMessage::CollabState {
            snapshot_base64, ..
        } = &state[0]
        else {
            panic!("{state:?}")
        };
        let human = LoroDoc::new();
        human.import(&B64.decode(snapshot_base64).unwrap()).unwrap();
        let original = human.oplog_vv();
        let body = human.get_text("body");
        body.insert(body.len_unicode(), "\nUser added 👩‍💻 שלום.")
            .unwrap();
        human.commit();
        let delta =
            human.export_json_updates_without_peer_compression(&original, &human.oplog_vv());
        let human_edit = ClientMessage::Act {
            id: "human-edit".into(),
            action: Action::ChangeRecord {
                request: engine::record_change::Request {
                    id: nucleus::new_uid("op"),
                    record_uid: uid.clone(),
                    mutation: engine::record_change::Mutation::Text {
                        update_base64: B64.encode(serde_json::to_vec(&delta).unwrap()),
                    },
                },
            },
        };
        let base = read(&tools, &uid).await;
        if human_first {
            user.handle(human_edit.clone()).await;
        }
        let mut events = engine.subscribe();
        let args = json!({"read_id":base["read_id"],"request_id":"agent-edit","edits":[
            {"field":"body","before":"Hello world.","after":"Hello from Fiote."},
            {"field":"head","before":"Shared 👩‍💻","after":"Shared 👩‍💻 with Fiote"}
        ]});
        let result = call(&tools, "lince_edit_text", args.clone()).await;
        assert_eq!(result["state"], "saved");
        assert_eq!(call(&tools, "lince_edit_text", args).await, result);
        let fact = events.recv().await.unwrap();
        let provenance: Value = serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
        assert_eq!(provenance["fiote"]["agent"], agent);
        assert_eq!(fact.cause.kind, nucleus::CauseKind::Fiote);
        assert!(
            user.on_fact(&fact)
                .await
                .iter()
                .any(|message| matches!(message, ServerMessage::CollabChange { .. }))
        );
        if !human_first {
            user.handle(human_edit).await;
        }
        let (head, body) = engine.doc_text(&uid).await.unwrap();
        assert_eq!(head, "Shared 👩‍💻 with Fiote");
        assert_eq!(
            body,
            "Hello from Fiote.\nKeep this paragraph.\nUser added 👩‍💻 שלום."
        );
        assert_eq!(read(&tools, &uid).await["record"]["body"], body);
    }
}

#[tokio::test]
async fn native_state_and_assignment_changes_reject_stale_properties_but_allow_live_typing() {
    let (engine, tools, uid, agent) = fixture().await;
    let lingua = engine
        .act(
            Action::CreateLingua {
                name: "Work".into(),
                visibility: "public".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for name in ["wip", "assigned-to"] {
        if store::concepts::resolve(&engine.store.pool, name)
            .await
            .unwrap()
            .is_none()
        {
            engine
                .act(
                    Action::CreateConcept {
                        lingua: lingua.clone(),
                        name: name.into(),
                        parents: vec![],
                    },
                    None,
                )
                .await
                .unwrap();
        }
    }
    let base = read(&tools, &uid).await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("Human is typing.".into()),
            },
            None,
        )
        .await
        .unwrap();
    call(
        &tools,
        "lince_action",
        json!({"request_id":"assign","read_ids":[base["read_id"]],"action":{
            "action":"assert-record","subject":uid,"predicate":"assigned-to","object":agent
        }}),
    )
    .await;
    let assigned = read(&tools, &uid).await;
    assert_eq!(assigned["record"]["assignees"][0]["uid"], agent);
    let stale = tools
        .run(
            "lince_action",
            json!({"request_id":"stale-quantity","read_ids":[base["read_id"]],"action":{
                "action":"set-quantity-exact","target":uid,"amount":"-3"
            }}),
        )
        .await;
    assert_eq!(stale["ok"], false);
    assert!(stale["error"].as_str().unwrap().contains("changed since"));
    let preview = call(&tools, "lince_action", json!({"request_id":"preview","read_ids":[],"action":{
        "action":"preview-area-transition","target":uid,"changes":{"quantity":"-3","assert":["wip"],"retract":[]}
    }})).await;
    call(
        &tools,
        "lince_action",
        json!({"request_id":"move","read_ids":[assigned["read_id"]],"action":{
            "action":"apply-area-transition","request_id":"move-to-wip","preview":preview["data"]
        }}),
    )
    .await;
    let current = read(&tools, &uid).await;
    assert_eq!(current["record"]["quantity_exact"], "-3");
    assert_eq!(current["record"]["body"], "Human is typing.");
    assert!(
        current["record"]["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["predicate"] == "wip")
    );
}

#[tokio::test]
async fn unsafe_text_replacement_bad_reads_and_ambiguous_edits_are_refused_without_changes() {
    let (engine, tools, uid, _) = fixture().await;
    let base = read(&tools, &uid).await;
    for args in [
        json!({"request_id":"overwrite","read_ids":[base["read_id"]],"action":{"action":"edit-record-text","target":uid,"body":"overwrite"}}),
        json!({"request_id":"no-read","read_ids":[],"action":{"action":"set-quantity-exact","target":uid,"amount":"7"}}),
    ] {
        assert_eq!(tools.run("lince_action", args).await["ok"], false);
    }
    for edits in [
        json!([{"field":"body","before":"missing","after":"x"}]),
        json!([{"field":"body","before":"","after":"x"}]),
        json!([{"field":"body","before":"Hello world.","after":"x"},{"field":"body","before":"world","after":"y"}]),
        json!([{"field":"quantity","before":"0","after":"7"}]),
    ] {
        assert_eq!(
            tools
                .run(
                    "lince_edit_text",
                    json!({"read_id":base["read_id"],"request_id":"invalid","edits":edits})
                )
                .await["ok"],
            false
        );
    }
    assert_eq!(
        engine.doc_text(&uid).await.unwrap().1,
        "Hello world.\nKeep this paragraph."
    );
    assert_eq!(read(&tools, &uid).await["record"]["quantity_exact"], "0");
}

#[tokio::test]
async fn native_queries_keep_the_sessions_visibility_and_cannot_elevate_its_actions() {
    let (engine, _, private, agent) = fixture().await;
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Reader".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, &person)
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::set_user_role(&engine.store.pool, &person, role)
        .await
        .unwrap();
    engine
        .act(
            Action::GrantVisibility {
                subject_kind: "actor".into(),
                subject: Some(person.clone()),
                target: agent.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let mut tools = Registry::default();
    Session::new(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "limited-fiote",
        Some(person),
    )
    .into_native_tools(Context {
        agent: agent.clone(),
        record: agent.clone(),
        thread: agent.clone(),
    })
    .register(&mut tools);
    let rows = call(
        &tools,
        "lince_query",
        json!({"protein":{"source":"record","where":[{"uid_eq":private}]}}),
    )
    .await;
    assert!(rows["rows"].as_array().unwrap().is_empty());
    assert_eq!(
        tools
            .run("lince_read_record", json!({"record_uid":private}))
            .await["ok"],
        false
    );
    let visible = read(&tools, &agent).await;
    assert_eq!(visible["record"]["uid"], agent);
    let denied = tools
        .run(
            "lince_action",
            json!({"request_id":"denied","read_ids":[visible["read_id"]],"action":{
                "action":"set-quantity-exact","target":agent,"amount":"9"
            }}),
        )
        .await;
    assert_eq!(denied["ok"], false);
    assert!(denied["error"].as_str().unwrap().contains("signed intent"));
    assert_eq!(tools.run("lince_sync_status", json!({})).await["ok"], false);
}

#[tokio::test]
async fn extension_edits_guard_the_data_read_and_retries_do_not_repeat_mutations() {
    let (engine, tools, uid, _) = fixture().await;
    let base = call(
        &tools,
        "lince_read_record",
        json!({"record_uid":uid,"extensions":["test.settings"]}),
    )
    .await;
    let action = json!({"request_id":"extension","read_ids":[base["read_id"]],"action":{
        "action":"set-extension","target":uid,"namespace":"test.settings","fds":{"value":1}
    }});
    let result = call(&tools, "lince_action", action.clone()).await;
    assert_eq!(call(&tools, "lince_action", action).await, result);
    let base = call(
        &tools,
        "lince_read_record",
        json!({"record_uid":uid,"extensions":["test.settings"]}),
    )
    .await;
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "test.settings".into(),
                fds: json!({"value":2}),
            },
            None,
        )
        .await
        .unwrap();
    let refused = tools
        .run(
            "lince_action",
            json!({"request_id":"stale-extension","read_ids":[base["read_id"]],"action":{
                "action":"set-extension","target":uid,"namespace":"test.settings","fds":{"value":3}
            }}),
        )
        .await;
    assert_eq!(refused["ok"], false);
    let current = call(
        &tools,
        "lince_read_record",
        json!({"record_uid":uid,"extensions":["test.settings"]}),
    )
    .await;
    assert_eq!(current["record"]["extensions"]["test.settings"]["value"], 2);
    let plain = read(&tools, &uid).await;
    let refused = tools
        .run(
            "lince_action",
            json!({"request_id":"unread-extension","read_ids":[plain["read_id"]],"action":{
                "action":"set-extension","target":uid,"namespace":"test.settings","fds":{"value":3}
            }}),
        )
        .await;
    assert_eq!(refused["ok"], false);
}

#[tokio::test]
async fn deleting_a_record_or_replacing_a_saved_query_requires_current_reads() {
    let (engine, tools, uid, _) = fixture().await;
    let base = read(&tools, &uid).await;
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("New human text".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        tools
            .run(
                "lince_action",
                json!({"request_id":"delete","read_ids":[base["read_id"]],"action":{
                    "action":"delete-record","target":uid
                }})
            )
            .await["ok"],
        false
    );
    let create = json!({"action":"save-protein","slug":"native-query","head":"Query","ast":{"source":"record","where":[],"limit":5}});
    let created = call(
        &tools,
        "lince_action",
        json!({"request_id":"new-protein","read_ids":[],"action":create}),
    )
    .await;
    assert_eq!(
        tools
            .run(
                "lince_action",
                json!({"request_id":"replace-unread","read_ids":[],"action":create})
            )
            .await["ok"],
        false
    );
    let base = call(
        &tools,
        "lince_read_record",
        json!({"record_uid":created["created"],"extensions":["lince.protein"]}),
    )
    .await;
    let mut update = create;
    update["ast"]["limit"] = json!(10);
    call(
        &tools,
        "lince_action",
        json!({"request_id":"replace-read","read_ids":[base["read_id"]],"action":update}),
    )
    .await;
    let result = call(&tools, "lince_query", json!({"saved":"native-query"})).await;
    assert_eq!(result["limit"], 10);
}

#[tokio::test]
async fn fiote_provenance_preserves_the_operators_signature_identity() {
    let (engine, tools, uid, agent) = fixture().await;
    let operator = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Operator".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .set_signer(engine::trust::Signer::generate(&operator, "operator-key"))
        .await
        .unwrap();
    let base = read(&tools, &uid).await;
    let mut events = engine.subscribe();
    call(
        &tools,
        "lince_edit_text",
        json!({"read_id":base["read_id"],"request_id":"signed-edit","edits":[
            {"field":"body","before":"Hello world.","after":"Fiote edited this."}
        ]}),
    )
    .await;
    let fact = events.recv().await.unwrap();
    assert_eq!(fact.actor_uid.as_deref(), Some(operator.as_str()));
    assert_eq!(fact.cause.kind, nucleus::CauseKind::Fiote);
    let payload: Value = serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
    assert_eq!(payload["fiote"]["agent"], agent);
    assert!(
        engine::trust::verify_fact(&engine.store, &fact)
            .await
            .unwrap()
    );
}
