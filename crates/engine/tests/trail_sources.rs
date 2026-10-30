mod support;

use engine::{Engine, actions::Action, sources::Command};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

async fn command(engine: &Engine, request: Command) -> Value {
    static DATA: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let data = DATA.get_or_init(|| tempfile::tempdir().unwrap());
    utils::config::set_lince_data_dir_override(data.path().join("lince")).unwrap();
    engine
        .act(Action::Sources { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn effects(engine: &Engine) {
    for _ in 0..20 {
        let result = engine.run_database_effects().await.unwrap();
        assert!(
            result.outcomes.iter().all(|outcome| outcome.ok),
            "{:?}",
            result.outcomes
        );
        if !result.pending {
            return;
        }
    }
    panic!("Authored rules did not settle");
}

#[test]
fn source_inspection_obeys_visibility_and_remote_commands_cannot_open_local_files() {
    support::run_async_test("source-access", || async {
        let engine = support::karma::engine().await;
        let person = support::person(&engine, "source-reader").await.uid;
        let role = store::auth::ensure_role(&engine.store.pool, "source-reader")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
        engine
            .act(
                Action::AssignRoles {
                    person: person.clone(),
                    roles: vec!["source-reader".into()],
                    expected_revision: 0,
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::SetRolePolicy {
                    role: "source-reader".into(),
                    expected_revision: 0,
                    policy: json!({"read":{"all":[]},"grants":[]}),
                },
                None,
            )
            .await
            .unwrap();
        let visible = support::plain(&engine, "visible-source", 0.0).await;
        let hidden = support::plain(&engine, "private-source", 0.0).await;
        let predicate = store::concepts::ensure(&engine.store.pool, "source-reference")
            .await
            .unwrap();
        engine
            .act(
                Action::AssertRecord {
                    subject: visible.clone(),
                    predicate,
                    object: Some(hidden.clone()),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        store::sqlx::query("INSERT INTO visibility_rule (uid,subject_kind,subject_uid,target_uid,grant_level) VALUES (?,'actor',?,?,'hidden')")
            .bind(nucleus::new_uid("v"))
            .bind(&person)
            .bind(&hidden)
            .execute(&engine.store.pool)
            .await
            .unwrap();
        let inspected = engine
            .act(
                Action::Sources {
                    request: Command::Inspect {
                        record: visible.clone(),
                    },
                },
                Some(person.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(inspected["related"].as_array().unwrap().is_empty());
        let local_uri = lince_resources::address("/source-access/private.mp4")
            .unwrap()
            .to_string();
        store::records::set_extension(
            &engine.store.pool,
            &visible,
            lince_resources::RESOURCE_NAMESPACE,
            &json!({"uri":local_uri,"media_type":"video/mp4","representations":[{"label":"Local copy","uri":local_uri,"media_type":"video/mp4"}]}),
        )
        .await
        .unwrap();
        for request in [
            Command::Inspect { record: hidden },
            Command::Preview {
                uri: local_uri.clone(),
                selected: vec![],
                bindings: BTreeMap::new(),
                refresh: false,
            },
            Command::Download {
                record: visible.clone(),
                representation: None,
            },
            Command::Download {
                record: visible.clone(),
                representation: Some(0),
            },
            Command::RetainInterval {
                record: visible.clone(),
                start_ms: 0,
                end_ms: 1000,
                representation: Some(0),
            },
            Command::Register {
                record: visible,
                path: "/source-access/private.mp4".into(),
            },
            Command::Export {
                uri: "https://example.invalid/Root.lingua".into(),
                selected: vec![],
                destination: "/source-access/export".into(),
                media: true,
            },
        ] {
            assert!(matches!(
                engine
                    .act(Action::Sources { request }, Some(person.clone()))
                    .await,
                Err(engine::EngineError::Forbidden(_))
            ));
        }
    });
}

#[test]
fn philosophy_content_is_selected_by_named_relations_without_personal_examples() {
    support::run_async_test("philosophy-query-content", || async {
        let engine = support::karma::engine().await;
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../institute/trails/scholazein/philosophy");
        let uri = repository
            .join("Philosophy.lingua")
            .to_string_lossy()
            .into_owned();
        let preview = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: vec![],
                bindings: BTreeMap::new(),
                refresh: true,
            },
        )
        .await;
        assert_eq!(preview["ready"], true);
        assert!(preview["rules"].as_array().unwrap().is_empty());
        let revision = preview["revision"].as_str().unwrap().to_owned();
        command(
            &engine,
            Command::Import {
                uri,
                selected: vec![],
                bindings: BTreeMap::new(),
                revision,
                update: false,
            },
        )
        .await;
        let unrelated = support::plain(&engine, "unrelated-geography", 0.0).await;
        let anchor: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE head=?")
            .bind("Ancient Greek philosophy: inquiry, knowledge, and living well")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        let source = store::concepts::ensure(&engine.store.pool, "source")
            .await
            .unwrap();
        engine
            .act(
                Action::AssertRecord {
                    subject: anchor,
                    predicate: source,
                    object: Some(unrelated.clone()),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        for name in ["philosophy", "ancient-greek-philosophy"] {
            let query: protein::Protein = serde_json::from_value(json!({
                "source": "record",
                "where": [{"relation": {"kind": name, "direction": "both"}}],
                "fields": ["uid", "head", "body", "assertions", "links"],
                "include": {"links": {"kinds": [name], "direction": "both", "depth": 1}}
            }))
            .unwrap();
            let rows = protein::execute(&engine.store, &query).await.unwrap();
            if name == "ancient-greek-philosophy" {
                assert_eq!(rows.len(), 65);
                assert!(rows.iter().all(|row| {
                    row["head"] != "Buddhism: from the path to sustained inquiry"
                        && row["head"] != "Yoga: an entrance through Light on Yoga"
                }));
            } else {
                for head in [
                    "Buddhism: from the path to sustained inquiry",
                    "Yoga: an entrance through Light on Yoga",
                ] {
                    assert!(rows.iter().any(|row| row["head"] == head));
                }
            }
            assert!(rows.iter().any(|row| row["head"] == "Philosophy"));
            assert!(
                rows.iter()
                    .any(|row| row["head"] == "Ancient Greek philosophy: research completion")
            );
            assert!(rows.iter().all(|row| row["uid"] != unrelated));
            assert!(rows.iter().all(|row| {
                row["head"] != "The First Philosophers of Greece"
                    && row["head"] != "Critical texts, scholarship, and academic benchmarks"
                    && row["head"] != "Apology, Crito, and Phaedo of Socrates"
            }));
            assert!(
                rows.iter()
                    .flat_map(|row| row["links"].as_array().unwrap())
                    .all(|link| link["kind"]
                        == if name == "philosophy" {
                            "philosophy"
                        } else {
                            "ancient-greek-philosophy"
                        })
            );
            for (from, to) in [
                (
                    "People, places, and chronology",
                    "Read a dialogue and reconstruct an argument",
                ),
                (
                    "Read a dialogue and reconstruct an argument",
                    "Apology: why examine a life?",
                ),
                (
                    "Apology: why examine a life?",
                    "Euthyphro: definitions, reasons, and approval",
                ),
                (
                    "Euthyphro: definitions, reasons, and approval",
                    "Crito: law, conscience, and a difficult decision",
                ),
                (
                    "Crito: law, conscience, and a difficult decision",
                    "Meno: can we learn what we do not yet know?",
                ),
                (
                    "Meno: can we learn what we do not yet know?",
                    "Gorgias: persuasion, power, and living well",
                ),
                (
                    "Gorgias: persuasion, power, and living well",
                    "Compare Plato and Xenophon",
                ),
                (
                    "Compare Plato and Xenophon",
                    "First review: reconstruct and challenge an argument",
                ),
                (
                    "The Milesians: what does an explanation explain?",
                    "Heraclitus: change, opposition, and intelligible order",
                ),
                (
                    "Parmenides: being, thought, and the force of an argument",
                    "The atomists: explaining change through what persists",
                ),
                (
                    "Seminar: unity, intellect, soul, and later Greek reception",
                    "Independent research: formulate and defend a contribution",
                ),
            ] {
                let first = rows.iter().find(|row| row["head"] == from).unwrap();
                let last = rows.iter().find(|row| row["head"] == to).unwrap();
                assert!(
                    first["links"].as_array().unwrap().iter().any(|link| {
                        link["from"] == first["uid"]
                            && link["to"] == last["uid"]
                            && link["direction"] == "out"
                    }),
                    "Missing learning continuation from {from} to {to}"
                );
            }
        }
        for (kind, included, excluded) in [
            (
                "buddhism",
                "Buddhism: from the path to sustained inquiry",
                "Yoga: an entrance through Light on Yoga",
            ),
            (
                "yoga",
                "Yoga: an entrance through Light on Yoga",
                "Buddhism: from the path to sustained inquiry",
            ),
        ] {
            let query: protein::Protein = serde_json::from_value(json!({
                "source": "record",
                "where": [{"relation": {"kind": kind, "direction": "both"}}],
                "fields": ["uid", "head", "links"],
                "include": {"links": {"kinds": [kind], "direction": "both", "depth": 1}}
            }))
            .unwrap();
            let rows = protein::execute(&engine.store, &query).await.unwrap();
            assert!(rows.iter().any(|row| row["head"] == included));
            assert!(rows.iter().all(|row| {
                row["head"] != excluded
                    && row["head"] != "Apology: why examine a life?"
                    && row["head"] != "Light on Yoga — B. K. S. Iyengar"
                    && row["head"] != "The Connected Discourses of the Buddha — Bodhi"
            }));
            if kind == "buddhism" {
                for head in [
                    "The path: view, intention, and ethical conduct",
                    "Buddhism: completing the core reading",
                    "Buddhism: a defended scholarly study",
                ] {
                    assert!(rows.iter().any(|row| row["head"] == head));
                }
                assert!(rows.iter().all(|row| {
                    row["head"] != "Read Light on Yoga: aims, discipline, and instruction"
                }));
            } else {
                assert!(rows.iter().any(|row| {
                    row["head"] == "Read Light on Yoga: aims, discipline, and instruction"
                }));
            }
        }
    });
}

#[test]
fn relative_text_and_query_references_open_without_importing_records() {
    support::run_async_test("relative-text-source", || async {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reading.json");
        std::fs::write(&path, "{\"source\":\"record\"}").unwrap();
        let engine = support::karma::engine().await;
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        let opened = command(
            &engine,
            Command::Open {
                reference: lince_resources::Reference {
                    target: "reading.json".into(),
                    base: Some(
                        lince_resources::Reference::path(&directory.path().join("Index.lingua"))
                            .unwrap()
                            .target,
                    ),
                    ..Default::default()
                },
                record: None,
            },
        )
        .await;
        assert_eq!(opened["ready"], false);
        assert!(
            opened["records"][0]["body"]
                .as_str()
                .unwrap()
                .contains("{\"source\":\"record\"}")
        );
        assert!(opened["records"][0]["uid"].is_null());
        for (hash, valid) in [
            (
                lince_resources::fingerprint(b"{\"source\":\"record\"}"),
                true,
            ),
            ("0".repeat(64), false),
        ] {
            let result = engine
                .act(
                    Action::Sources {
                        request: Command::Open {
                            reference: lince_resources::Reference {
                                target: lince_resources::Reference::path(&path).unwrap().target,
                                hash: Some(hash),
                                ..Default::default()
                            },
                            record: None,
                        },
                    },
                    None,
                )
                .await;
            assert_eq!(result.is_ok(), valid);
        }
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
    });
}

#[test]
fn source_inspection_exposes_incoming_assertions_without_creating_reverse_links() {
    support::run_async_test("incoming-source-links", || async {
        let engine = support::karma::engine().await;
        let first = support::plain(&engine, "first-reading", 0.0).await;
        let last = support::plain(&engine, "last-reading", 0.0).await;
        let predicate = store::concepts::ensure(&engine.store.pool, "ancient-greek-philosophy")
            .await
            .unwrap();
        engine
            .act(
                Action::AssertRecord {
                    subject: first.clone(),
                    predicate: predicate.clone(),
                    object: Some(last.clone()),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        let inspected = command(
            &engine,
            Command::Inspect {
                record: last.clone(),
            },
        )
        .await;
        let related = inspected["related"].as_array().unwrap();
        assert!(
            related
                .iter()
                .any(|link| link["uid"] == first && link["direction"] == "in")
        );
        assert!(
            store::assertions::for_subjects(&engine.store.pool, &[last])
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store::assertions::edges_of_predicate(&engine.store.pool, &predicate)
                .await
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn source_import_preserves_personal_state_and_recomputes_authored_assertions() {
    support::run_async_test("trail-sources", || async {
        let temporary = tempfile::tempdir().unwrap();
        let engine = support::karma::engine().await;
        let learner = support::person(&engine, "learner").await.uid;
        let other = support::person(&engine, "other-learner").await.uid;
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
        let publication = temporary.path().join("publication");
        std::fs::create_dir_all(publication.join("philosophy")).unwrap();
        std::fs::create_dir_all(publication.join("shared")).unwrap();
        for name in [
            "shared/Development-example.lingua",
            "shared/Checklist-example.lingua",
            "shared/Assessment-example.lingua",
        ] {
            std::fs::copy(repository.join(name), publication.join(name)).unwrap();
        }
        let mut pending = vec![repository.join("philosophy")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else if entry.file_type().unwrap().is_file() {
                    let path = entry.path();
                    let destination = publication.join(path.strip_prefix(&repository).unwrap());
                    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                    std::fs::copy(path, destination).unwrap();
                }
            }
        }
        let source_path = publication.join("Route.lingua");
        std::fs::write(&source_path, "Import \"./shared/Development-example.lingua\"\nImport \"./philosophy/Philosophy.lingua\"\nImport \"./philosophy/ancient-greek-philosophy/Philosophers.lingua\"\nImport \"./philosophy/ancient-greek-philosophy/plato/Plato.lingua\"\n").unwrap();
        let uri = source_path.to_string_lossy().into_owned();
        let selected = vec![];
        let bindings = BTreeMap::from([("learner".into(), learner.clone())]);
        let preview = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                refresh: true,
            },
        )
        .await;
        assert_eq!(preview["ready"], true);
        assert_eq!(preview["binding_ready"], false);
        let revision = preview["revision"].as_str().unwrap().to_owned();
        let imported = command(
            &engine,
            Command::Import {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                revision: revision.clone(),
                update: false,
            },
        )
        .await;
        let binding = imported["binding"].as_str().unwrap().to_owned();
        let ready = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                refresh: false,
            },
        )
        .await;
        assert_eq!(ready["binding_ready"], true);
        let row = preview["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["slug"] == "reading-state")
            .unwrap();
        let reading = row["uid"].as_str().unwrap().to_owned();
        let definitions = store::records::get_extension(
            &engine.store.pool,
            &binding,
            engine::sources::BINDING_NAMESPACE,
        )
        .await
        .unwrap()
        .unwrap();
        let mastery = definitions["concepts"]["mastery"].as_str().unwrap();
        let pool = &engine.store.pool;
        let assertion_value = |person: String, predicate: String| async move {
            store::assertions::for_subjects(pool, &[person])
                .await
                .unwrap()
                .into_iter()
                .find(|assertion| assertion.predicate_uid == predicate)
                .map(|assertion| assertion.quantity.unwrap().to_string())
        };
        command(
            &engine,
            Command::Activate {
                record: binding.clone(),
            },
        )
        .await;
        for (quantity, expected) in [("1", "100"), ("1", "100"), ("0", "0"), ("1", "100")] {
            engine
                .act(
                    Action::SetQuantityExact {
                        target: reading.clone(),
                        amount: quantity.into(),
                    },
                    None,
                )
                .await
                .unwrap();
            effects(&engine).await;
            assert_eq!(
                assertion_value(learner.clone(), mastery.into())
                    .await
                    .as_deref(),
                Some(expected)
            );
        }
        assert!(
            assertion_value(other.clone(), mastery.into())
                .await
                .is_none()
        );
        let again = command(
            &engine,
            Command::Import {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                revision: revision.clone(),
                update: true,
            },
        )
        .await;
        assert_eq!(again["created"], 0);
        assert_eq!(
            store::records::quantity(&engine.store.pool, &reading)
                .await
                .unwrap()
                .unwrap()
                .to_string(),
            "1"
        );
        command(
            &engine,
            Command::Activate {
                record: binding.clone(),
            },
        )
        .await;
        assert_eq!(
            store::assertions::for_subjects(&engine.store.pool, std::slice::from_ref(&learner))
                .await
                .unwrap()
                .into_iter()
                .filter(|assertion| assertion.predicate_uid == mastery)
                .count(),
            1
        );
        let second = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: BTreeMap::from([("learner".into(), other.clone())]),
                refresh: false,
            },
        )
        .await;
        let second_reading = second["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["slug"] == "reading-state")
            .unwrap()["uid"]
            .as_str()
            .unwrap();
        assert_ne!(reading, second_reading);
        let alternate = publication.join("Alternate.lingua");
        std::fs::write(&alternate, "Import \"./shared/Development-example.lingua\"\nAnother reading route (@alternate: 0, #contains @philosophy) { r_00000000000000000000009991\n} r_00000000000000000000009991\n").unwrap();
        let shared = command(
            &engine,
            Command::Preview {
                uri: alternate.to_string_lossy().into_owned(),
                selected: vec![],
                bindings: bindings.clone(),
                refresh: true,
            },
        )
        .await;
        let shared_mastery = shared["vocabulary"]
            .as_array()
            .unwrap()
            .iter()
            .find(|concept| concept["name"] == "mastery")
            .unwrap();
        assert_eq!(shared_mastery["uid"], mastery);
        let plato = preview["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["slug"] == "plato")
            .unwrap()["uid"]
            .as_str()
            .unwrap();
        engine
            .act(
                Action::EditRecordText {
                    target: plato.into(),
                    head: None,
                    body: Some("My own argument notes".into()),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::EditRecordText {
                    target: reading.clone(),
                    head: None,
                    body: Some("Personal reading evidence".into()),
                },
                None,
            )
            .await
            .unwrap();
        let original = std::fs::read_to_string(&source_path).unwrap();
        std::fs::create_dir_all(publication.join("notes")).unwrap();
        std::fs::write(publication.join("notes/Notes.lingua"), "Publisher notes (@publisher-notes: 0) { r_00000000000000000000009992\nAdditional references in a later source revision.\n} r_00000000000000000000009992\n").unwrap();
        std::fs::write(
            &source_path,
            format!("{original}Import \"./notes/Notes.lingua\"\n"),
        )
        .unwrap();
        let philosophy = publication.join("philosophy/Philosophy.lingua");
        std::fs::write(
            &philosophy,
            std::fs::read_to_string(&philosophy).unwrap().replace(
                "Philosophy (@philosophy:",
                "Philosophy revised (@philosophy:",
            ),
        )
        .unwrap();
        let dialogues =
            publication.join("philosophy/ancient-greek-philosophy/sources/Apology-Cary.lingua");
        std::fs::write(
            &dialogues,
            std::fs::read_to_string(&dialogues)
                .unwrap()
                .replace("fragment \"pgepubid00003\"", "fragment \"revised-passage\""),
        )
        .unwrap();
        let updated = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                refresh: true,
            },
        )
        .await;
        assert_eq!(updated["binding"], binding);
        assert_eq!(updated["binding_ready"], false);
        assert!(
            updated["records"]
                .as_array()
                .unwrap()
                .iter()
                .any(|record| record["slug"] == "apology" && record["reference_review"] == true)
        );
        command(
            &engine,
            Command::Import {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: bindings.clone(),
                revision: updated["revision"].as_str().unwrap().into(),
                update: true,
            },
        )
        .await;
        assert_eq!(
            store::records::get(&engine.store.pool, plato)
                .await
                .unwrap()
                .unwrap()
                .body,
            "My own argument notes"
        );
        let personal = store::records::get(&engine.store.pool, &reading)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(personal.body, "Personal reading evidence");
        assert_eq!(personal.quantity.to_string(), "1");
        assert_eq!(
            assertion_value(learner.clone(), mastery.into()).await,
            Some("100".into())
        );
        effects(&engine).await;
        assert_eq!(
            assertion_value(learner.clone(), mastery.into()).await,
            Some("100".into())
        );
        let plain = command(
            &engine,
            Command::Preview {
                uri: philosophy.to_string_lossy().into_owned(),
                selected: vec![],
                bindings: BTreeMap::new(),
                refresh: false,
            },
        )
        .await;
        assert!(plain["rules"].as_array().unwrap().is_empty());
        let checklist = command(
            &engine,
            Command::Preview {
                uri: publication
                    .join("shared/Checklist-example.lingua")
                    .to_string_lossy()
                    .into_owned(),
                selected: vec![],
                bindings: bindings.clone(),
                refresh: false,
            },
        )
        .await;
        assert!(
            checklist["rules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|rule| !rule.to_string().contains("mastery"))
        );
        let imported_checklist = command(
            &engine,
            Command::Import {
                uri: publication
                    .join("shared/Checklist-example.lingua")
                    .to_string_lossy()
                    .into_owned(),
                selected: vec![],
                bindings,
                revision: checklist["revision"].as_str().unwrap().into(),
                update: false,
            },
        )
        .await;
        let checklist_binding = imported_checklist["binding"].as_str().unwrap();
        let definitions = store::records::get_extension(
            &engine.store.pool,
            checklist_binding,
            engine::sources::BINDING_NAMESPACE,
        )
        .await
        .unwrap()
        .unwrap();
        let checked = definitions["concepts"]["checked"].as_str().unwrap();
        let checklist_state = checklist["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["slug"] == "checklist-state")
            .unwrap()["uid"]
            .as_str()
            .unwrap();
        command(
            &engine,
            Command::Activate {
                record: checklist_binding.into(),
            },
        )
        .await;
        for expected in ["1", "1", "0"] {
            engine
                .act(
                    Action::SetQuantityExact {
                        target: checklist_state.into(),
                        amount: expected.into(),
                    },
                    None,
                )
                .await
                .unwrap();
            effects(&engine).await;
            assert_eq!(
                assertion_value(learner.clone(), checked.into()).await,
                Some(expected.into())
            );
        }
        assert_eq!(
            assertion_value(learner.clone(), mastery.into()).await,
            Some("100".into())
        );
        let uri = publication
            .join("shared/Assessment-example.lingua")
            .to_string_lossy()
            .into_owned();
        let selected = vec![];
        let preview = command(
            &engine,
            Command::Preview {
                uri: uri.clone(),
                selected: selected.clone(),
                bindings: BTreeMap::new(),
                refresh: false,
            },
        )
        .await;
        let imported = command(
            &engine,
            Command::Import {
                uri,
                selected,
                bindings: BTreeMap::new(),
                revision: preview["revision"].as_str().unwrap().into(),
                update: false,
            },
        )
        .await;
        let records = preview["records"].as_array().unwrap();
        let response = records
            .iter()
            .find(|record| record["slug"] == "assessment-response")
            .unwrap()["uid"]
            .as_str()
            .unwrap();
        let points = records
            .iter()
            .find(|record| record["slug"] == "assessment-points")
            .unwrap()["uid"]
            .as_str()
            .unwrap();
        let controls =
            store::records::get_extension(&engine.store.pool, response, "lince.controls")
                .await
                .unwrap()
                .unwrap();
        let answer: Action =
            serde_json::from_value(controls["actions"][0]["action"].clone()).unwrap();
        let clear: Action =
            serde_json::from_value(controls["actions"][1]["action"].clone()).unwrap();
        command(
            &engine,
            Command::Activate {
                record: imported["binding"].as_str().unwrap().into(),
            },
        )
        .await;
        for (action, expected) in [
            (answer.clone(), "5"),
            (answer.clone(), "5"),
            (clear, "0"),
            (answer, "5"),
        ] {
            engine.act(action, None).await.unwrap();
            effects(&engine).await;
            assert_eq!(
                store::facts::level(&engine.store.pool, points)
                    .await
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    });
}

#[test]
fn linked_documents_browse_without_cell_writes_and_resolve_after_independent_imports() {
    support::run_async_test("linked-source-browsing", || async {
        let engine = support::karma::engine().await;
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("Parent.lingua");
        let child = directory.path().join("Child.lingua");
        std::fs::write(&parent, "Parent (@parent: 0, #contains \"./Child.lingua#r_00000000000000000000009982\") { r_00000000000000000000009981\n} r_00000000000000000000009981\n").unwrap();
        std::fs::write(&child, "Child (@child: 0, #part-of \"./Parent.lingua#r_00000000000000000000009981\") { r_00000000000000000000009982\n} r_00000000000000000000009982\n").unwrap();
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        let browse = command(
            &engine,
            Command::Open {
                reference: lince_resources::Reference::path(&child).unwrap(),
                record: None,
            },
        )
        .await;
        assert_eq!(browse["browsing"], true);
        assert_eq!(browse["records"].as_array().unwrap().len(), 1);
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        let mut imported = Vec::new();
        for path in [&child, &parent] {
            let uri = path.to_string_lossy().into_owned();
            let preview = command(
                &engine,
                Command::Preview {
                    uri: uri.clone(),
                    selected: vec![],
                    bindings: BTreeMap::new(),
                    refresh: false,
                },
            )
            .await;
            let uid = preview["records"][0]["uid"].as_str().unwrap().to_owned();
            let result = command(
                &engine,
                Command::Import {
                    uri,
                    selected: vec![],
                    bindings: BTreeMap::new(),
                    revision: preview["revision"].as_str().unwrap().into(),
                    update: false,
                },
            )
            .await;
            assert!(result["binding"].is_null());
            imported.push(uid);
        }
        let links = store::assertions::for_subjects(&engine.store.pool, &imported)
            .await
            .unwrap();
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.object_uid.is_none()
            && link.object_uri.is_some()
            && link.resolved_object_uid.is_some()));
        let reread = command(
            &engine,
            Command::Open {
                reference: lince_resources::Reference::new(&imported[0]),
                record: None,
            },
        )
        .await;
        assert_eq!(reread["local_record"], imported[0]);
        support::plain(&engine, "outside-this-source", 0.0).await;
        for (base, record) in [
            (
                Some(lince_resources::Reference::path(&child).unwrap().target),
                None,
            ),
            (None, Some(imported[0].clone())),
        ] {
            assert!(
                engine
                    .act(
                        Action::Sources {
                            request: Command::Open {
                                reference: lince_resources::Reference {
                                    target: "outside-this-source".into(),
                                    base,
                                    ..Default::default()
                                },
                                record,
                            },
                        },
                        None
                    )
                    .await
                    .is_err()
            );
        }
    });
}
