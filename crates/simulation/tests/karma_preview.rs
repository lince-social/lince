use std::future::Future;

use chrono::{DateTime, Utc};
use engine::{
    Engine,
    actions::Action,
    karma_preview::{Input, Limits, ProposedRule, Report, Request},
};
use nucleus::karma::rule_field::RuleFieldInput;
use nucleus::simulation::{
    CheckDefinition, CheckOptions, Comparison, Evaluation, Predicate, Quantity, Stop,
};

#[allow(dead_code)]
mod karma {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../engine/tests/support/karma.rs"));
}

fn run(test: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(test);
        })
        .unwrap()
        .join()
        .unwrap();
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(1_893_456_000_000).unwrap()
}

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    karma::authorize(&engine).await;
    simulation::karma_preview::install(&engine).unwrap();
    for slug in ["stock", "other"] {
        engine
            .act_at(
                Action::CreateRecord {
                    slug: Some(slug.into()),
                    head: slug.into(),
                    body: String::new(),
                    kind: nucleus::RecordKind::Plain,
                    quantity: 0.0,
                },
                None,
                now(),
            )
            .await
            .unwrap();
    }
    engine
}

fn proposal(condition: &str, consequence: &str) -> ProposedRule {
    let condition = if condition.contains('@') {
        condition.to_owned()
    } else {
        format!("0 * @other + ({condition})")
    };
    ProposedRule {
        identity: None,
        rule: None,
        expected_revision: None,
        fields: [condition.as_str(), "always", consequence].map(|source| RuleFieldInput::Text {
            source: source.into(),
        }),
    }
}

fn request(proposals: Vec<ProposedRule>) -> Request {
    Request {
        proposals,
        limits: Limits {
            horizon_ms: 1000,
            rule_evaluations: 50,
            ..Default::default()
        },
        inputs: vec![Input::Occurrence {
            after_ms: 0,
            proposal: 0,
        }],
        records: vec!["stock".into()],
        quantity_basis: Default::default(),
        checks: Vec::new(),
        saved_checks: None,
        checks_start_ms: None,
        checking: Default::default(),
    }
}

#[test]
fn component_presentation_preview_records_incomplete_external_coverage_without_live_pushes() {
    run(async {
        let engine = engine().await;
        let mut components = engine.subscribe_components();
        let before = engine.store.state_hash().await.unwrap();
        let report = preview(&engine, request(vec![proposal("1", "@stock: show({\"kind\":\"record\",\"mode\":\"call\"})")])).await;
        assert!(report.incomplete);
        assert_eq!(report.stop, Stop::UnsupportedEffect { cell: "current".into() });
        assert!(components.try_recv().is_err());
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

async fn preview(engine: &Engine, request: Request) -> Report {
    serde_json::from_value(
        engine
            .act_at(Action::PreviewKarmaProposal { request }, None, now())
            .await
            .unwrap()
            .data
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn command_queries_and_consequences_use_controlled_responses_in_a_private_copy() {
    run(async {
        let engine = engine().await;
        let command = engine.act_at(Action::SaveKarmaCommand {
            target: None, expected_revision: None, slug: "reader".into(), head: "Reader".into(),
            configuration: nucleus::command::Command::Shell { script: "exit 99".into() }, host: None,
        }, None, now()).await.unwrap().created.unwrap();
        let before = engine.store.state_hash().await.unwrap();
        let mut query = request(vec![proposal("query_command(@reader)", "@stock")]);
        query.inputs.insert(0, Input::CommandResponse { after_ms: 0, response: nucleus::command::CommandResponse { command: command.clone(), ok: true, stdout: "-2.75".into(), stderr: String::new() } });
        let report = preview(&engine, query).await;
        assert!(!report.incomplete, "{:?}", report.unsupported);
        assert_eq!(report.final_values[0].quantity.as_ref().unwrap().value.to_string(), "-2.75");
        let report = preview(&engine, request(vec![proposal("query_command(@reader)", "@stock")])).await;
        assert!(report.incomplete);
        assert_eq!(report.final_values[0].quantity.as_ref().unwrap().value.to_string(), "0");
        let mut consequence = request(vec![proposal("1", "@stock: run(@reader)")]);
        consequence.inputs.insert(0, Input::CommandResponse { after_ms: 0, response: nucleus::command::CommandResponse { command, ok: true, stdout: "done".into(), stderr: String::new() } });
        assert!(!preview(&engine, consequence).await.incomplete);
        let report = preview(&engine, request(vec![proposal("1", "@stock: run(@reader)")])).await;
        assert!(report.incomplete);
        assert_eq!(report.stop, Stop::UnsupportedEffect { cell: "current".into() });
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        let invocations: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_command_invocation").fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(invocations, 0);
    });
}

#[test]
fn an_unsaved_edit_runs_in_a_copy_and_a_changed_proposal_has_a_new_fingerprint() {
    run(async {
        let engine = engine().await;
        let initial = proposal("1", "@stock = -9");
        let uid = engine
            .act_at(initial.action("original".into()), None, now())
            .await
            .unwrap()
            .created
            .unwrap();
        let mut draft = proposal("2", "@stock");
        draft.rule = Some(uid.clone());
        draft.expected_revision = Some(1);
        let before = engine.store.state_hash().await.unwrap();
        let report = preview(&engine, request(vec![draft.clone()])).await;
        assert_eq!(
            report.final_values[0].quantity.as_ref().unwrap().value,
            store::exact::integer(2)
        );
        assert_eq!(report.stop, Stop::HorizonReached {});
        assert!(!report.incomplete);
        assert!(report.source_current);
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(
            store::recurrence::get(&engine.store.pool, &uid)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        draft.fields[0] = RuleFieldInput::Text {
            source: "0 * @other + 3".into(),
        };
        assert_ne!(request(vec![draft]).fingerprint().unwrap(), report.draft);
    });
}

#[test]
fn an_authorized_actor_can_save_and_preview_in_copied_data() {
    run(async {
        let engine = engine().await;
        let person = actor(&engine, true).await;
        let before = engine.store.state_hash().await.unwrap();
        let report: Report = serde_json::from_value(
            engine
                .act_at(
                    Action::PreviewKarmaProposal {
                        request: request(vec![proposal("-1", "@stock")]),
                    },
                    Some(person),
                    now(),
                )
                .await
                .unwrap()
                .data
                .unwrap(),
        )
        .unwrap();
        assert!(report.source_current, "{report:?}");
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert!(
            store::recurrence::all(&engine.store.pool)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!report.incomplete, "{report:?}");
        assert_eq!(report.evaluations, 1);
        assert_eq!(
            report.final_values[0].quantity.as_ref().unwrap().value,
            store::exact::integer(-1)
        );
    });
}

#[test]
fn a_proposed_rule_reports_the_first_broken_restriction_and_actual_stopping_value() {
    run(async {
        let engine = engine().await;
        let mut request = request(vec![proposal("-1", "@stock")]);
        request.checks.push(CheckDefinition {
            id: "stock-floor".into(),
            predicate: Predicate::Quantity {
                cell: "current".into(),
                record: "stock".into(),
                comparison: Comparison::AtLeast,
                expected: Quantity {
                    value: store::exact::zero(),
                    unit: None,
                },
            },
            options: CheckOptions {
                evaluation: Evaluation::EveryChange,
                name: "Stock must remain nonnegative".into(),
                ..Default::default()
            },
        });
        let report = preview(&engine, request).await;
        assert_eq!(
            report.stop,
            Stop::CheckFailed {
                check: "stock-floor".into()
            }
        );
        assert_eq!(report.first_failure.unwrap().check.id, "stock-floor");
        assert_eq!(
            report.final_values[0].quantity.as_ref().unwrap().value,
            store::exact::integer(-1)
        );
        assert!(report.incomplete);
    });
}

#[test]
fn an_unmet_threshold_is_a_complete_preview_without_a_change() {
    run(async {
        let engine = engine().await;
        let mut draft = proposal("-1", "@stock");
        draft.fields[1] = RuleFieldInput::Text {
            source: ">0".into(),
        };
        let report = preview(&engine, request(vec![draft])).await;
        assert_eq!(report.stop, Stop::HorizonReached {});
        assert_eq!(report.evaluations, 1);
        assert!(
            report.final_values[0]
                .quantity
                .as_ref()
                .unwrap()
                .value
                .is_zero()
        );
        assert!(report.unsupported.is_empty());
        assert!(!report.incomplete);
    });
}

#[test]
fn rising_feedback_stops_inside_the_chain_and_names_its_rules() {
    run(async {
        let engine = engine().await;
        let mut request = request(vec![
            proposal("@stock + 1", "@other"),
            proposal("@other + 1", "@stock"),
        ]);
        request.inputs = vec![Input::Quantity {
            after_ms: 1,
            record: "stock".into(),
            value: store::exact::integer(1),
        }];
        let report = preview(&engine, request).await;
        assert_eq!(report.stop, Stop::RuleEvaluationBudget {});
        assert_eq!(report.evaluations, 50);
        assert!(!report.cycles.is_empty());
        assert_eq!(report.cycles[0].rules.len(), 2);
        assert!(report.incomplete);
        assert!(
            store::records::resolve(&engine.store.pool, "stock")
                .await
                .unwrap()
                .unwrap()
                .quantity
                .is_zero()
        );
    });
}

#[test]
fn native_tools_run_an_isolated_proposal_and_replay_only_unchanged_source_data() {
    run(async {
        let engine = std::sync::Arc::new(engine().await);
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
        let stock = store::records::resolve(&engine.store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let mut tools = fiote::tools::Registry::default();
        cell::Session::local(
            engine.clone(),
            std::sync::Arc::new(cell::LaneHub::new()),
            "karma-tools",
        )
        .into_native_tools(cell::FioteContext {
            agent,
            record: stock.clone(),
            thread: stock.clone(),
        })
        .register(&mut tools);
        let before = engine.store.state_hash().await.unwrap();
        let action = Action::PreviewKarmaProposal {
            request: request(vec![proposal("-1", "@stock")]),
        };
        let arguments = serde_json::json!({"request_id":"proposal","action":serde_json::to_string(&action).unwrap(),"read_ids":[]});
        let first = tools.run("lince_action", arguments.clone()).await;
        assert_eq!(first["ok"], true, "{first}");
        assert_eq!(
            first["result"]["data"]["final_values"][0]["quantity"]["value"]["value"],
            "-1"
        );
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
        assert_eq!(tools.run("lince_action", arguments.clone()).await, first);
        engine
            .act(
                Action::SetQuantityExact {
                    target: stock,
                    amount: "3".into(),
                },
                None,
            )
            .await
            .unwrap();
        let stale = tools.run("lince_action", arguments).await;
        assert_eq!(stale["ok"], false);
        assert!(stale.to_string().contains("Source data changed"));
    });
}

#[test]
fn a_signed_preview_retains_verification_and_replay_checks_without_blocking_live_edits() {
    run(async {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        let engine = std::sync::Arc::new(engine().await);
        let person = actor(&engine, true).await;
        let signer = engine::trust::Signer::generate(&person, "preview-key");
        let mut session = cell::Session::new(
            engine.clone(),
            std::sync::Arc::new(cell::LaneHub::new()),
            "signed-preview",
            Some(person.clone()),
        );
        let cell::ServerMessage::SessionChallenge {
            session_id,
            challenge,
            ..
        } = session.initialize_action_intent().await
        else {
            panic!("missing challenge")
        };
        let public_key_base64 = signer.public_key_b64();
        let signature = signer.sign_bytes(&nucleus::action_intent::session_authentication_bytes(
            &session_id,
            &challenge,
            &person,
            "preview-key",
            &public_key_base64,
        ));
        let registered = session
            .handle(cell::ClientMessage::SessionAuthenticate {
                id: "auth".into(),
                session_id: session_id.clone(),
                session_challenge: challenge.clone(),
                person_uid: person,
                key_id: "preview-key".into(),
                public_key_base64,
                signature,
            })
            .await;
        assert!(
            matches!(
                registered.as_slice(),
                [cell::ServerMessage::SessionAuthenticated { .. }]
            ),
            "{registered:?}"
        );
        let action = Action::PreviewKarmaProposal {
            request: request(vec![proposal("-1", "@stock")]),
        };
        let unsigned = session
            .handle(cell::ClientMessage::Act {
                id: "unsigned".into(),
                action: action.clone(),
            })
            .await;
        assert!(
            matches!(unsigned.as_slice(), [cell::ServerMessage::Error { code: Some(code), .. }] if code == "action_intent_required")
        );
        let action_base64 = B64.encode(serde_json::to_vec(&action).unwrap());
        let signature = signer.sign_bytes(&nucleus::action_intent::signing_bytes(
            &session_id,
            &challenge,
            1,
            "preview",
            &action_base64,
        ));
        let signed = cell::ClientMessage::SignedAct {
            id: "preview".into(),
            session_id,
            session_challenge: challenge,
            sequence: 1,
            action_base64,
            signature,
        };
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        engine
            .install_karma_preview_runner(std::sync::Arc::new(HeldResult {
                ready: ready.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let request = session.handle(signed.clone());
        let edit = async {
            tokio::time::timeout(std::time::Duration::from_secs(10), ready.notified())
                .await
                .unwrap();
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                engine.act(
                    Action::SetQuantityExact {
                        target: "other".into(),
                        amount: "5".into(),
                    },
                    None,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            release.notify_one();
        };
        let (response, ()) = tokio::join!(request, edit);
        assert!(
            matches!(response.as_slice(), [cell::ServerMessage::ActionOk { data: Some(data), .. }] if data["source_current"] == false && data["incomplete"] == false && data["evaluations"] == 1),
            "{response:?}"
        );
        simulation::karma_preview::install(&engine).unwrap();
        let cell::ClientMessage::SignedAct {
            session_id,
            session_challenge,
            action_base64,
            ..
        } = &signed
        else {
            unreachable!()
        };
        let signature = signer.sign_bytes(&nucleus::action_intent::signing_bytes(
            session_id,
            session_challenge,
            2,
            "fresh-preview",
            action_base64,
        ));
        let fresh = session
            .handle(cell::ClientMessage::SignedAct {
                id: "fresh-preview".into(),
                session_id: session_id.clone(),
                session_challenge: session_challenge.clone(),
                sequence: 2,
                action_base64: action_base64.clone(),
                signature,
            })
            .await;
        assert!(
            matches!(fresh.as_slice(), [cell::ServerMessage::ActionOk { data: Some(data), .. }] if data["source_current"] == true && data["incomplete"] == false && data["evaluations"] == 1 && data["final_values"][0]["quantity"]["value"]["value"] == "-1"),
            "{fresh:?}"
        );
        let replay = session.handle(signed).await;
        assert!(
            matches!(replay.as_slice(), [cell::ServerMessage::Error { code: Some(code), .. }] if code == "action_intent_replay")
        );
    });
}

#[test]
fn a_tool_preview_keeps_other_tools_and_connection_closure_available() {
    run(async {
        let engine = std::sync::Arc::new(engine().await);
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
        let stock = store::records::resolve(&engine.store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let native = cell::Session::local(
            engine.clone(),
            std::sync::Arc::new(cell::LaneHub::new()),
            "preview-concurrency",
        )
        .into_native_tools(cell::FioteContext {
            agent,
            record: stock.clone(),
            thread: stock,
        });
        let mut tools = fiote::tools::Registry::default();
        native.register(&mut tools);
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        engine
            .install_karma_preview_runner(std::sync::Arc::new(HeldResult {
                ready: ready.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let action = Action::PreviewKarmaProposal {
            request: request(vec![proposal("-1", "@stock")]),
        };
        let arguments = serde_json::json!({"request_id":"preview-held","action":serde_json::to_string(&action).unwrap(),"read_ids":[]});
        let preview = tools.run("lince_action", arguments.clone());
        let control = async {
            tokio::time::timeout(std::time::Duration::from_secs(10), ready.notified())
                .await
                .unwrap();
            let replay = tools.run("lince_action", arguments).await;
            assert_eq!(replay["ok"], false);
            assert!(replay.to_string().contains("still running"));
            let collision = tools.run("lince_action", serde_json::json!({"request_id":"preview-held","action":serde_json::to_string(&Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Plain, head: "Collision".into(), body: String::new(), quantity: 0.0 }).unwrap(),"read_ids":[]})).await;
            assert_eq!(collision["ok"], false);
            assert!(collision.to_string().contains("different operation"));
            let described = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                tools.run(
                    "lince_describe",
                    serde_json::json!({"action":"save-karma-rule"}),
                ),
            )
            .await
            .unwrap();
            assert_eq!(described["ok"], true);
            tokio::time::timeout(std::time::Duration::from_secs(3), native.close())
                .await
                .unwrap();
            release.notify_one();
        };
        let (result, ()) = tokio::join!(preview, control);
        assert_eq!(result["ok"], false);
        assert!(result.to_string().contains("closed"));
    });
}

#[test]
fn an_interrupted_tool_preview_releases_its_slot_and_keeps_a_refusal_receipt() {
    run(async {
        let engine = std::sync::Arc::new(engine().await);
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
        let stock = store::records::resolve(&engine.store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let mut tools = fiote::tools::Registry::default();
        cell::Session::local(
            engine.clone(),
            std::sync::Arc::new(cell::LaneHub::new()),
            "preview-interrupted",
        )
        .into_native_tools(cell::FioteContext {
            agent,
            record: stock.clone(),
            thread: stock,
        })
        .register(&mut tools);
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        engine
            .install_karma_preview_runner(std::sync::Arc::new(HeldResult {
                ready: ready.clone(),
                release: std::sync::Arc::new(tokio::sync::Notify::new()),
            }))
            .unwrap();
        let action = Action::PreviewKarmaProposal {
            request: request(vec![proposal("-1", "@stock")]),
        };
        let arguments = serde_json::json!({"request_id":"preview-interrupted","action":serde_json::to_string(&action).unwrap(),"read_ids":[]});
        let mut preview = Box::pin(tools.run("lince_action", arguments.clone()));
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::select! {
                result = &mut preview => panic!("Preview returned before interruption: {result}"),
                () = ready.notified() => {},
            }
        })
        .await
        .unwrap();
        drop(preview);
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let result = tools.run("lince_action", arguments.clone()).await;
                assert_eq!(result["ok"], false);
                if result.to_string().contains("interrupted") {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });
}

#[test]
fn a_command_consequence_is_incomplete_and_cannot_escape_the_copy() {
    run(async {
        let engine = engine().await;
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("escaped");
        let command = serde_json::to_string(&format!("touch {}", path.display())).unwrap();
        let report = preview(
            &engine,
            request(vec![proposal("1", &format!("@stock: command({command})"))]),
        )
        .await;
        assert_eq!(
            report.stop,
            Stop::UnsupportedEffect {
                cell: "current".into()
            }
        );
        assert!(report.incomplete);
        assert!(!path.exists());
    });
}

#[test]
fn an_empty_assumption_list_does_not_pretend_saving_a_rule_fired_it() {
    run(async {
        let engine = engine().await;
        let mut request = request(vec![proposal("-1", "@stock")]);
        request.inputs.clear();
        let report = preview(&engine, request).await;
        assert!(
            report.final_values[0]
                .quantity
                .as_ref()
                .unwrap()
                .value
                .is_zero()
        );
        assert_eq!(report.evaluations, 0);
    });
}

#[test]
fn controlled_execution_cannot_recursively_start_a_preview() {
    run(async {
        let engine = engine().await;
        let execution = nucleus::execution::Execution::new([7; 32], now().timestamp_millis())
            .unwrap()
            .controlled(
                nucleus::execution::control::Control::new(50, None, None),
                "copy".into(),
            );
        let result = execution
            .scope(engine.act(
                Action::PreviewKarmaProposal {
                    request: request(vec![proposal("1", "@stock")]),
                },
                None,
            ))
            .await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot start another")
        );
    });
}

async fn actor(engine: &Engine, can_create: bool) -> String {
    let uid = engine
        .act_at(
            Action::CreateRecord {
                slug: Some("reader".into()),
                head: "Reader".into(),
                body: String::new(),
                kind: nucleus::RecordKind::Person,
                quantity: 1.0,
            },
            None,
            now(),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "preview-reader")
        .await
        .unwrap();
    let mut permissions = vec![
        ("record", "read"),
        ("record", "update"),
        ("frequency", "read"),
    ];
    if can_create {
        permissions.push(("frequency", "create"));
        permissions.push(("frequency", "update"));
    }
    for (resource, action) in permissions {
        let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &uid, "reader", "hash", role)
        .await
        .unwrap();
    for name in ["stock", "other"] {
        let record = store::records::resolve(&engine.store.pool, name)
            .await
            .unwrap()
            .unwrap();
        store::visibility::grant(&engine.store.pool, "actor", Some(&uid), &record.uid)
            .await
            .unwrap();
    }
    uid
}

#[test]
fn a_reader_cannot_use_simulation_to_author_an_unauthorized_rule() {
    run(async {
        let engine = engine().await;
        let actor = actor(&engine, false).await;
        let before = engine.store.state_hash().await.unwrap();
        let result = engine
            .act_at(
                Action::PreviewKarmaProposal {
                    request: request(vec![proposal("1", "@stock")]),
                },
                Some(actor),
                now(),
            )
            .await;
        assert!(matches!(result, Err(engine::EngineError::Forbidden(_))));
        assert_eq!(engine.store.state_hash().await.unwrap(), before);
    });
}

#[test]
fn nested_value_inputs_remain_private_even_when_the_outer_record_is_readable() {
    run(async {
        let engine = engine().await;
        engine
            .act_at(
                Action::CreateRecord {
                    slug: Some("secret".into()),
                    head: "Secret".into(),
                    body: String::new(),
                    kind: nucleus::RecordKind::Plain,
                    quantity: 321.0,
                },
                None,
                now(),
            )
            .await
            .unwrap();
        engine
            .act_at(
                proposal("@secret", "@other").action("private-value".into()),
                None,
                now(),
            )
            .await
            .unwrap();
        let actor = actor(&engine, true).await;
        let outcome = engine
            .act_at(
                Action::PreviewKarmaProposal {
                    request: request(vec![proposal("value(@other)", "@stock")]),
                },
                Some(actor),
                now(),
            )
            .await
            .unwrap();
        let report: Report = serde_json::from_value(outcome.data.unwrap()).unwrap();
        assert!(report.incomplete);
        assert!(!report.unsupported.is_empty());
        assert!(
            report.final_values[0]
                .quantity
                .as_ref()
                .unwrap()
                .value
                .is_zero()
        );
    });
}

struct HeldResult {
    ready: std::sync::Arc<tokio::sync::Notify>,
    release: std::sync::Arc<tokio::sync::Notify>,
}

impl engine::karma_preview::Runner for HeldResult {
    fn run<'a>(
        &'a self,
        engine: &'a Engine,
        actor: Option<String>,
        request: Request,
        now: DateTime<Utc>,
    ) -> engine::karma_preview::PreviewFuture<'a> {
        Box::pin(async move {
            let report = simulation::karma_preview::run(engine, actor.as_deref(), request, now)
                .await
                .map_err(engine::karma_preview::invalid)?;
            self.ready.notify_one();
            self.release.notified().await;
            Ok(report)
        })
    }
}

#[test]
fn access_revoked_after_running_is_checked_before_returning_the_report() {
    run(async {
        let engine = engine().await;
        let actor = actor(&engine, true).await;
        let stock = store::records::resolve(&engine.store.pool, "stock")
            .await
            .unwrap()
            .unwrap()
            .uid;
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        engine
            .install_karma_preview_runner(std::sync::Arc::new(HeldResult {
                ready: ready.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let preview = engine.act_at(
            Action::PreviewKarmaProposal {
                request: request(vec![proposal("1", "@stock")]),
            },
            Some(actor),
            now(),
        );
        let revoke = async {
            tokio::time::timeout(std::time::Duration::from_secs(10), ready.notified())
                .await
                .unwrap();
            store::sqlx::query("DELETE FROM visibility_rule WHERE target_uid = ?")
                .bind(stock)
                .execute(&engine.store.pool)
                .await
                .unwrap();
            release.notify_one();
        };
        let (result, ()) = tokio::join!(preview, revoke);
        assert!(matches!(result, Err(engine::EngineError::Forbidden(_))));
    });
}
