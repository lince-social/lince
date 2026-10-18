use super::*;
use nucleus::trust_analysis::claims::{
    Comparator, Descriptor, Metric,
    commands::{Command as ClaimCommand, Draft, Review},
};

fn request(person: &Person, id: &str, request: ClaimCommand) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: id.into(),
        request: Command::Claims { request },
    }
}

pub(super) async fn check_publisher_amount_against_original_receipts(
    reader: &Engine,
    person: &Person,
    sources: &Query,
    original: &Value,
) {
    let measurement = serde_json::from_value(original["groups"][0]["measurement"].clone()).unwrap();
    let draft = Draft {
        issuer: Identity::Person(person.uid.clone()),
        claim: None,
        title: "A publisher's contribution claim".into(),
        descriptor: Descriptor {
            subject: sources.subject.clone(),
            role: sources.role,
            measurement,
            level: nucleus::trust_analysis::query::EvidenceLevel::NeedLinkedReceiver,
            metric: Metric::Quantity,
            counterparty_basis: sources.counterparty_basis,
            from: None,
            until: None,
        },
        comparator: Comparator::AtLeast,
        amount: "100".into(),
        as_of: None,
        lifetime_seconds: 3600,
        expected_revision: 0,
        withdrawn: false,
        audience: nucleus::trust_analysis::contacts::Audience::Public,
        key_id: person.signer.key_id.clone(),
        public_key: person.signer.public_key_b64(),
    };
    let preview = reader
        .act(
            request(
                person,
                "analysis:claim-preview",
                ClaimCommand::Preview { draft },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let mut review: Review = serde_json::from_value(preview["trust"]["review"].clone()).unwrap();
    engine::trust_analysis::sign_claim_document(&mut review.document, &person.signer).unwrap();
    let snapshot = serde_json::to_string(&review.document).unwrap();
    let retained_before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record").fetch_one(&reader.store.pool).await.unwrap();
    let snapshot_check = reader.act(request(person, "analysis:unretained-snapshot", ClaimCommand::CheckDocument { document:snapshot, sources:sources.clone(), as_organ:false }), Some(person.uid.clone())).await.unwrap();
    assert!(snapshot_check.created.is_none());
    assert!(snapshot_check.facts.is_empty());
    let snapshot_check = snapshot_check.data.unwrap();
    assert_eq!(snapshot_check["trust"]["support"]["observed"]["value"], "12.000000000000000001");
    assert_eq!(snapshot_check["trust"]["support"]["state"], "partial_support");
    let retained_after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record").fetch_one(&reader.store.pool).await.unwrap();
    assert_eq!(retained_before, retained_after);
    let expected_review = serde_json::from_value(preview["trust"]["hash"].clone()).unwrap();
    let record = reader
        .act(
            request(
                person,
                "analysis:claim-publish",
                ClaimCommand::Publish {
                    review,
                    expected_review,
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    let mut weighted = sources.clone();
    weighted.weights.need_linked_receiver = DecimalValue::parse_inferred("0.1").unwrap();
    let check = reader
        .act(
            request(
                person,
                "analysis:claim-check",
                ClaimCommand::Check {
                    record: record.clone(),
                    sources: weighted,
                    as_organ: false,
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert!(check.facts.is_empty());
    assert!(check.created.is_none());
    let checked = &check.data.unwrap()["trust"];
    assert_eq!(checked["support"]["claimed"]["value"], "100");
    assert_eq!(
        checked["support"]["observed"]["value"],
        "12.000000000000000001"
    );
    assert_eq!(checked["support"]["state"], "partial_support");
    assert_eq!(checked["support"]["observed_only"], true);
    assert_eq!(
        checked["analysis"]["groups"][0]["weighted_index"]["value"],
        "1.200000000000000000"
    );
    assert_eq!(
        checked["query"]["issuers"],
        serde_json::to_value(&sources.issuers).unwrap()
    );
    let mut excluded = sources.clone();
    excluded
        .issuers
        .exclude
        .extend(sources.issuers.include.clone());
    let unknown = reader
        .act(
            request(
                person,
                "analysis:claim-excluded",
                ClaimCommand::Check {
                    record,
                    sources: excluded,
                    as_organ: false,
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(unknown["trust"]["support"]["state"], "unknown");
    assert!(unknown["trust"]["support"]["observed"].is_null());
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let occurrences: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    assert_eq!(occurrences, 0);
}
