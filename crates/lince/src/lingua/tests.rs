use super::*;

#[test]
fn pure_scripts_ignore_unavailable_database_directories() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("absent");
    run_source(Path::new("calculation.lingua"), "dir = \"not-an-instance\"; let x = 0.1 + 0.2; print(x == 0.3); print(false && quantity(@missing));", Some(missing.clone())).unwrap();
    assert!(!missing.exists());
}

#[test]
fn script_paths_resolve_directory_literals_beside_the_script() {
    let document = anicca::parse("dir = \"../instance\"; print(1);").unwrap();
    assert_eq!(
        configuration(&document, Path::new("/data/scripts/report.lingua")).unwrap(),
        Some(PathBuf::from("/data/scripts/../instance"))
    );
    let duplicate = anicca::parse("dir = \"a\"; dir = \"b\";").unwrap();
    assert!(configuration(&duplicate, Path::new("report.lingua")).is_err());
}

#[test]
fn missing_or_invalid_database_errors_name_the_effective_path() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = data::DataHost::new(Some(directory.path().into()));
    let error = host
        .call(
            "quantity",
            &[nucleus::script::Value::Reference("apples".into())],
        )
        .unwrap_err();
    assert!(error.contains(&directory.path().join("lince.db").display().to_string()));
    assert!(!directory.path().join("lince.db").exists());
    std::fs::write(directory.path().join("lince.db"), b"invalid").unwrap();
    assert!(
        host.call(
            "quantity",
            &[nucleus::script::Value::Reference("apples".into())]
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(directory.path().join("lince.db")).unwrap(),
        b"invalid"
    );
}

#[test]
fn extension_reads_keep_proteins_private_namespace_restrictions() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = data::DataHost::new(Some(directory.path().into()));
    let error = host
        .call(
            "extension",
            &[
                nucleus::script::Value::Reference("apples".into()),
                nucleus::script::Value::Text("lince.social.private".into()),
            ],
        )
        .unwrap_err();
    assert!(error.contains("Private social state"), "{error}");
    assert!(!directory.path().join("lince.db").exists());
}

fn populated_instance(directory: &Path, amount: &str) -> String {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let store = store::Store::open(&format!(
            "sqlite://{}",
            directory.join("lince.db").display()
        ))
        .await
        .unwrap();
        let row = store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: Some("apples"),
                kind: nucleus::RecordKind::Plain,
                head: "Apples",
                body: "Fruit.",
                quantity: nucleus::DecimalValue::parse_inferred(amount).unwrap(),
            },
        )
        .await
        .unwrap();
        store.pool.close().await;
        row.uid
    })
}

#[test]
fn record_readings_and_protein_queries_select_the_requested_instance_exactly() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let uid = populated_instance(first.path(), "9007199254740993");
    populated_instance(second.path(), "2");
    let mut host = data::DataHost::new(Some(first.path().into()));
    let reference = nucleus::script::Value::Reference("apples".into());
    assert_eq!(
        host.call("quantity", std::slice::from_ref(&reference))
            .unwrap()
            .render(),
        "9007199254740993"
    );
    let printed = host
        .call("record", std::slice::from_ref(&reference))
        .unwrap()
        .render();
    let projected = anicca::project(&anicca::parse(&printed).unwrap()).unwrap();
    assert_eq!(projected.records[0].uid, uid);
    assert_eq!(projected.records[0].head, "Apples");
    assert_eq!(
        projected.records[0].quantity.as_ref().unwrap().0,
        "9007199254740993"
    );
    let query = nucleus::script::Value::from_json(serde_json::json!({"source":"record", "where":[{"slug_eq":"apples"}], "fields":["head","quantity"], "limit": 5})).unwrap();
    let result = host.call("query", &[query]).unwrap().render();
    assert!(result.contains("9007199254740993"));
    assert!(result.contains("Apples"));
    let query = nucleus::script::Value::from_json(serde_json::json!({"source":"record", "where":[{"quantity_eq":9007199254740993_i64}], "fields":["quantity"], "limit": 5})).unwrap();
    let result = host.call("query", &[query]).unwrap().render();
    assert!(result.contains("9007199254740993"));
    for _ in 0..300 {
        assert_eq!(
            host.call("quantity", std::slice::from_ref(&reference))
                .unwrap()
                .render(),
            "9007199254740993"
        );
    }
    let mut other = data::DataHost::new(Some(second.path().into()));
    assert_eq!(other.call("quantity", &[reference]).unwrap().render(), "2");
}

#[test]
fn checking_and_formatting_scripts_never_opens_their_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("script.lingua");
    std::fs::write(&path, "dir = \"missing\"; print(quantity(@apples));").unwrap();
    dispatch(&["check".into(), path.display().to_string()]).unwrap();
    dispatch(&["fmt".into(), "--write".into(), path.display().to_string()]).unwrap();
    assert!(!directory.path().join("missing").exists());
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("quantity(@apples)")
    );
}

#[test]
fn saved_feature_declarations_print_without_running_their_definitions() {
    use nucleus::karma::*;
    use nucleus::script::Value;
    let directory = tempfile::tempdir().unwrap();
    let uid = populated_instance(directory.path(), "2");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rule_uid = runtime.block_on(async {
        let store = store::Store::open(&format!(
            "sqlite://{}",
            directory.path().join("lince.db").display()
        ))
        .await
        .unwrap();
        store::records::set_extension(
            &store.pool,
            &uid,
            "sample",
            &serde_json::json!({"score": 7}),
        )
        .await
        .unwrap();
        let now = nucleus::execution::now();
        let rule = store::recurrence::create(
            &store.pool,
            store::recurrence::NewRecurrence {
                record_uid: &uid,
                consequences: Consequences::capture(
                    nucleus::DecimalValue::parse_inferred("-1").unwrap(),
                    None,
                ),
                condition: None,
                note: Some("Only print this definition"),
                cadence: Cadence::every_months(1),
                anchor_at: now,
                request_id: "lingua-print-rule",
                actor_uid: None,
            },
            now,
        )
        .await
        .unwrap();
        let duration = |value| DurationBinding::Literal {
            value: DurationMs::new(value),
        };
        store::karma::frequencies::create(
            &store.pool,
            store::karma::frequencies::CreateFrequencyInput {
                request_id: "lingua-print-frequency".into(),
                frequency: FrequencyAst {
                    schema: FrequencySchema::V1,
                    slug: Slug::new("print-clock").unwrap(),
                    purpose: "Print clock".into(),
                    tags: Default::default(),
                    parameters: Default::default(),
                    cadence: FrequencyCadenceAst::Elapsed {
                        interval: duration(1_000),
                        anchor: TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap(),
                    },
                    timer: FrequencyTimerAst {
                        required_resolution: duration(1),
                        max_lateness: duration(5),
                        coalesce_window: duration(0),
                    },
                    missed: MissedPolicy::Replay {
                        max: std::num::NonZeroU32::new(64).unwrap(),
                    },
                    inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
                    rephase: RephasePolicy::PreserveAnchor,
                    overload: OverloadPolicy::PauseAndAsk,
                },
                owner_person_uid: None,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap();
        let rule_uid = rule.rule().uid.clone();
        store.pool.close().await;
        rule_uid
    });
    let mut host = data::DataHost::new(Some(directory.path().into()));
    let extension = host
        .call(
            "extension",
            &[
                Value::Reference("apples".into()),
                Value::Text("sample".into()),
            ],
        )
        .unwrap()
        .render();
    let projected = anicca::project(&anicca::parse(&extension).unwrap()).unwrap();
    assert_eq!(projected.extensions[0].fields["score"], 7);
    let query = Value::from_json(serde_json::json!({"source":"record", "where":[{"slug_eq":"apples"}], "include":{"extension":{"namespace":"sample"}}})).unwrap();
    let Value::List(rows) = host.call("query", &[query]).unwrap() else {
        panic!("query rows expected")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("query projection expected")
    };
    assert_eq!(row["extension"].to_json().unwrap()["score"], 7);
    let rule = host
        .call("rule", &[Value::Reference(rule_uid.clone())])
        .unwrap()
        .render();
    let projected = anicca::project(&anicca::parse(&rule).unwrap()).unwrap();
    assert_eq!(projected.rules[0].uid, rule_uid);
    assert_eq!(projected.rules[0].record_slug, uid);
    assert!(projected.rules[0].cadence.is_some());
    let frequency = host
        .call("frequency", &[Value::Reference("print-clock".into())])
        .unwrap()
        .render();
    let projected = anicca::project(&anicca::parse(&frequency).unwrap()).unwrap();
    let definition: FrequencyAst =
        serde_json::from_str(projected.frequencies[0].definition.as_ref().unwrap()).unwrap();
    assert_eq!(definition.purpose, "Print clock");
    assert_eq!(projected.frequencies[0].quantity, 0);
    assert_eq!(
        host.call("quantity", &[Value::Reference("apples".into())])
            .unwrap()
            .render(),
        "2"
    );
}
