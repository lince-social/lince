use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::RuleFieldInput;
use serde_json::json;

mod support;

fn text(source: &str) -> RuleFieldInput {
    RuleFieldInput::Text {
        source: source.into(),
    }
}

async fn save(engine: &Engine, source: &str, target: &str) -> String {
    engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [text(source), text("always"), text(&format!("@{target}"))],
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn set(engine: &Engine, target: &str, fields: serde_json::Value) {
    engine
        .act(
            Action::SetExtension {
                target: target.into(),
                namespace: "shop.inventory".into(),
                fds: fields,
            },
            None,
        )
        .await
        .unwrap();
}

async fn quantity(engine: &Engine, record: &str) -> String {
    let value = store::facts::level(&engine.store.pool, record)
        .await
        .unwrap()
        .to_string();
    if value.contains('.') {
        value.trim_end_matches('0').trim_end_matches('.').into()
    } else {
        value
    }
}

#[tokio::test]
async fn prices_wake_rules_without_quantity_changes_and_bind_to_the_original_record() {
    let engine = support::engine().await;
    let stock = support::plain(&engine, "stock", 10.0).await;
    let total = support::plain(&engine, "total", 0.0).await;
    set(&engine, &stock, json!({"price":1.25,"label":"apples"})).await;
    let rule = save(
        &engine,
        "@stock * extension(@stock, \"shop.inventory\", \"price\")",
        "total",
    )
    .await;
    set(&engine, &stock, json!({"price":2.75})).await;
    assert_eq!(quantity(&engine, &stock).await, "10");
    assert_eq!(quantity(&engine, &total).await, "27.5");
    engine
        .act(
            Action::SetSlug {
                target: stock.clone(),
                slug: Some("renamed".into()),
            },
            None,
        )
        .await
        .unwrap();
    support::plain(&engine, "stock", 100.0).await;
    set(&engine, &stock, json!({"price":3.125})).await;
    assert_eq!(quantity(&engine, &total).await, "31.25");
    let stored = store::recurrence::get(&engine.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored
            .condition
            .unwrap()
            .bindings
            .iter()
            .all(|binding| binding.target.as_str() == stock)
    );
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":stock}],"include":{"numeric_extensions":true}}),
    )
    .unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(
        rows[0]["numeric_extensions"],
        json!([{"namespace":"shop.inventory","property":"price","value":"3.125"}])
    );
    let field = store::karma_fields::for_rule(&engine.store.pool, &rule)
        .await
        .unwrap()
        .into_iter()
        .find(|field| field.kind == nucleus::karma::rule_field::RuleFieldKind::Condition)
        .unwrap();
    engine
        .act(
            Action::ReviseKarmaField {
                field: field.uid,
                expected_revision: field.revision,
                source: "@stock * extension(@stock, \"shop.inventory\", \"weight\")".into(),
                request_id: nucleus::new_uid("request"),
            },
            None,
        )
        .await
        .unwrap();
    set(&engine, &stock, json!({"price":100,"weight":4.25})).await;
    assert_eq!(quantity(&engine, &total).await, "42.5");
}

#[tokio::test]
async fn unavailable_properties_fail_explicitly_and_leave_the_previous_result_unchanged() {
    let engine = support::engine().await;
    let stock = support::plain(&engine, "stock", 1.0).await;
    let total = support::plain(&engine, "total", 9.0).await;
    let source = "extension(@stock, \"shop.inventory\", \"price\")";
    let rule = save(&engine, source, "total").await;
    for fields in [
        json!({"other":1}),
        json!({"price":"0"}),
        json!({"price":false}),
        json!({"price":null}),
        json!({"price":[]}),
    ] {
        set(&engine, &stock, fields).await;
        let error = engine
            .act(
                Action::PreviewKarmaReading {
                    source: source.into(),
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("missing") || error.to_string().contains("JSON number"),
            "{error}"
        );
        assert_eq!(quantity(&engine, &total).await, "9");
    }
    let failed: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE rule_uid = ? AND status = 'failed'",
    )
    .bind(rule)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert!(failed >= 5);
    set(&engine, &stock, json!({"price":0})).await;
    assert_eq!(quantity(&engine, &total).await, "0");
}

#[tokio::test]
async fn stored_json_numbers_keep_decimal_precision_and_support_exponents() {
    let engine = support::engine().await;
    let record = support::plain(&engine, "numbers", 1.0).await;
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'precise', ?)",
    )
    .bind(&record)
    .bind(
        r#"{"price":1234567890123456789.123456789,"tiny":2.5e-7,"large":2.5e3,"too_small":1e-30}"#,
    )
    .execute(&engine.store.pool)
    .await
    .unwrap();
    for (property, expected) in [
        ("price", "1234567890123456789.123456789"),
        ("tiny", "0.00000025"),
        ("large", "2500"),
    ] {
        let result = engine
            .act(
                Action::PreviewKarmaReading {
                    source: format!("extension(@numbers, \"precise\", \"{property}\")"),
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.data.unwrap()["value"], expected);
    }
    assert!(
        engine
            .act(
                Action::PreviewKarmaReading {
                    source: "extension(@numbers, \"precise\", \"too_small\")".into()
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn private_properties_are_unavailable_in_previews_and_native_choices() {
    let engine = support::engine().await;
    let secret = support::plain(&engine, "secret", 0.0).await;
    set(&engine, &secret, json!({"price":17})).await;
    let person = support::person(&engine, "reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "extension-reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(
        &engine.store.pool,
        &person.uid,
        "extension-reader",
        "hash",
        role,
    )
    .await
    .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &secret)
        .await
        .unwrap();
    let visible = engine
        .act(
            Action::PreviewKarmaReading {
                source: "extension(@secret, \"shop.inventory\", \"price\")".into(),
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert_eq!(visible.data.unwrap()["value"], "17");
    store::sqlx::query("DELETE FROM visibility_rule WHERE target_uid = ?")
        .bind(&secret)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::PreviewKarmaReading {
                    source: "extension(@secret, \"shop.inventory\", \"price\")".into(),
                },
                Some(person.uid.clone())
            )
            .await
            .is_err()
    );
    let query =
        serde_json::from_value(json!({"source":"record","include":{"numeric_extensions":true}}))
            .unwrap();
    let rows = protein::execute_for(&engine.store, &query, Some(&person.uid))
        .await
        .unwrap();
    assert!(rows.iter().all(|row| row["uid"] != secret));
}

#[tokio::test]
async fn received_extension_updates_wake_local_rules_once_after_sync() {
    let source = support::engine().await;
    let receiver = support::engine().await;
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    source
        .set_signer(engine::trust::Signer::generate(&organ, "extensions"))
        .await
        .unwrap();
    receiver
        .adopt_introduction(&source.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    let stock = support::plain(&source, "stock", 10.0).await;
    set(&source, &stock, json!({"price":1})).await;
    let (ops, checkpoint) = source.ops_after(0, 100_000).await.unwrap();
    receiver
        .import_op_batch(&engine::sync::OpBatch {
            from_organ: organ.clone(),
            ops,
        })
        .await
        .unwrap();
    let total = support::plain(&receiver, "total", 0.0).await;
    save(
        &receiver,
        &format!("extension(@{stock}, \"shop.inventory\", \"price\")"),
        "total",
    )
    .await;
    set(&source, &stock, json!({"price":7.25})).await;
    let (ops, _) = source.ops_after(checkpoint, 100_000).await.unwrap();
    let batch = engine::sync::OpBatch {
        from_organ: organ,
        ops,
    };
    receiver.import_op_batch(&batch).await.unwrap();
    assert_eq!(quantity(&receiver, &total).await, "7.25");
    assert_eq!(quantity(&receiver, &stock).await, "10");
    assert_eq!(receiver.import_op_batch(&batch).await.unwrap(), 0);
    assert_eq!(quantity(&receiver, &total).await, "7.25");
}
