use super::*;
use nucleus::DecimalValue;
use nucleus::trust_analysis::{Identity, commands::Command, query::Query};
#[path = "analysis/claims.rs"]
mod claims;
#[path = "analysis/recipes.rs"]
mod recipes;

pub(super) async fn inspect_original_outcomes(
    reader: &Engine,
    person: &Person,
    giver: &str,
    receiver: &str,
    original: &Engine,
) {
    let role = store::auth::ensure_role(&reader.store.pool, "Evidence analyst")
        .await
        .unwrap();
    for operation in ["read", "create"] {
        let permission = store::auth::ensure_permission(&reader.store.pool, "record", operation)
            .await
            .unwrap();
        store::auth::grant(&reader.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(
        &reader.store.pool,
        &person.uid,
        &person.uid,
        "fixture",
        role,
    )
    .await
    .unwrap();
    let mut query = Query::for_subject(Identity::Organ(giver.into()));
    query
        .issuers
        .include
        .push(nucleus::trust_analysis::Condition {
            identities: vec![Identity::Organ(receiver.into())],
            ..Default::default()
        });
    assert!(
        !reader
            .may_read_record(Some(&person.uid), giver)
            .await
            .unwrap()
    );
    let hidden = reader
        .act(
            Action::TrustAnalysis {
                person: Some(person.uid.clone()),
                request_id: "analysis:hidden-profile".into(),
                request: Command::AnalyzeQuery {
                    query: query.clone(),
                    as_organ: false,
                },
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert!(
        hidden["trust"]["analysis"]["groups"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for profile in std::collections::BTreeSet::from([giver, receiver]) {
        store::replica::make_own_root(&reader.store.pool, profile)
            .await
            .unwrap();
        store::visibility::grant(&reader.store.pool, "actor", Some(&person.uid), profile)
            .await
            .unwrap();
    }
    assert!(
        reader
            .may_read_record(Some(&person.uid), giver)
            .await
            .unwrap()
    );
    let owned:Vec<String> = store::sqlx::query_scalar("SELECT record_uid FROM transfer_receipt_record WHERE owner_person_uid=? AND kind='publication'").bind(&person.uid).fetch_all(&reader.store.pool).await.unwrap();
    assert!(!owned.is_empty());
    for record in owned {
        assert!(
            reader
                .may_read_record(Some(&person.uid), &record)
                .await
                .unwrap(),
            "The receipt owner must be able to inspect its retained original"
        );
    }
    let changed = reader.watch_query_changes();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    let result = reader
        .act(
            Action::TrustAnalysis {
                person: Some(person.uid.clone()),
                request_id: "analysis:original".into(),
                request: Command::AnalyzeQuery {
                    query: query.clone(),
                    as_organ: false,
                },
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert!(result.facts.is_empty());
    assert!(result.created.is_none());
    assert!(!changed.has_changed().unwrap());
    let data = result.data.unwrap();
    let analysis = &data["trust"]["analysis"];
    assert_eq!(
        analysis["groups"].as_array().unwrap().len(),
        1,
        "{analysis:#}"
    );
    assert_eq!(analysis["groups"][0]["level"], "need_linked_receiver");
    assert_eq!(
        analysis["groups"][0]["raw_quantity"]["value"],
        "12.000000000000000001"
    );
    assert_eq!(analysis["groups"][0]["occurrences"], 1);
    assert_eq!(analysis["partial"], false);
    assert_eq!(analysis["rows"].as_array().unwrap().len(), 2);
    assert!(
        analysis["protocol"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["content"]["kind"] == "need"
                && row["content"]["requested"]["value"] == "10")
    );
    assert!(analysis["protocol"].as_array().unwrap().iter().any(|row| {
        row["content"]["kind"] == "settled"
            && serde_json::from_value::<nucleus::transfer::receipt::RecordedFloat>(
                row["content"]["quantity"].clone(),
            )
            .unwrap()
            .value()
            .unwrap()
                == 10.0
    }));
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    claims::check_publisher_amount_against_original_receipts(reader, person, &query, analysis)
        .await;
    recipes::automatic_summary_uses_public_receiver_evidence(reader, person, &query, analysis, original)
        .await;
    let mut weighted = query.clone();
    weighted.weights.need_linked_receiver = DecimalValue::parse_inferred("0.25").unwrap();
    let result = reader
        .act(
            Action::TrustAnalysis {
                person: Some(person.uid.clone()),
                request_id: "analysis:weighted".into(),
                request: Command::AnalyzeQuery {
                    query: weighted.clone(),
                    as_organ: false,
                },
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let group = &result["trust"]["analysis"]["groups"][0];
    assert_eq!(group["raw_quantity"]["value"], "12.000000000000000001");
    assert_eq!(group["weighted_index"]["value"], "3.000000000000000000");
    assert_eq!(group["index_rounded"], true);
    let save = Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: "analysis:save".into(),
        request: Command::SaveQuery {
            name: "My local food analysis".into(),
            query: weighted.clone(),
        },
    };
    let saved = reader
        .act(save.clone(), Some(person.uid.clone()))
        .await
        .unwrap();
    assert_eq!(saved.facts.len(), 1);
    let record = saved.created.unwrap();
    let replayed = reader.act(save, Some(person.uid.clone())).await.unwrap();
    assert!(replayed.facts.is_empty());
    assert_eq!(replayed.created.as_deref(), Some(record.as_str()));
    let result = reader
        .act(
            Action::TrustAnalysis {
                person: Some(person.uid.clone()),
                request_id: "analysis:load".into(),
                request: Command::LoadQuery {
                    record: record.clone(),
                },
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(
        serde_json::from_value::<Query>(result["trust"]["query"].clone()).unwrap(),
        weighted
    );
    let other = support::person(reader, "analysis.other").await;
    store::auth::create_credential(&reader.store.pool, &other.uid, &other.uid, "fixture", role)
        .await
        .unwrap();
    assert!(
        reader
            .act(
                Action::TrustAnalysis {
                    person: Some(other.uid.clone()),
                    request_id: "analysis:other".into(),
                    request: Command::LoadQuery { record },
                },
                Some(other.uid.clone())
            )
            .await
            .is_err()
    );
    assert!(
        reader
            .act(
                Action::TrustAnalysis {
                    person: Some(other.uid.clone()),
                    request_id: "analysis:spoof".into(),
                    request: Command::AnalyzeQuery {
                        query,
                        as_organ: false
                    },
                },
                Some(person.uid.clone())
            )
            .await
            .is_err()
    );
    reader
        .act(
            Action::SetRolePolicy {
                role: "Evidence analyst".into(),
                expected_revision: 0,
                policy: json!({"read":{"all":[]},"grants":[]}),
            },
            None,
        )
        .await
        .unwrap();
    let before:(i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM trust_query_record)").fetch_one(&reader.store.pool).await.unwrap();
    let mut denied = weighted;
    denied.weights.giver = DecimalValue::parse_inferred("0.2").unwrap();
    assert!(
        reader
            .act(
                Action::TrustAnalysis {
                    person: Some(person.uid.clone()),
                    request_id: "analysis:denied-create".into(),
                    request: Command::SaveQuery {
                        name: "Denied bounded creation".into(),
                        query: denied
                    },
                },
                Some(person.uid.clone())
            )
            .await
            .is_err()
    );
    let after:(i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM trust_query_record)").fetch_one(&reader.store.pool).await.unwrap();
    assert_eq!(before, after);
}
