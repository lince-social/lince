use super::*;
use nucleus::trust_analysis::{
    claims::{self as model, commands::Command as ClaimCommand},
    query::Query,
};

fn command(person: &support::Person, id: &str, request: ClaimCommand) -> Action {
    action(person, id, Command::Claims { request })
}

async fn check(
    node: &Engine,
    person: &support::Person,
    document: &model::Document,
    id: &str,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    node.act(
        command(
            person,
            id,
            ClaimCommand::CheckDocument {
                document: serde_json::to_string(document).unwrap(),
                sources: Query::for_subject(document.statement.descriptor.subject.clone()),
                as_organ: false,
            },
        ),
        Some(person.uid.clone()),
    )
    .await
}

#[test]
fn snapshot_checks_require_recognized_authority_and_create_no_retained_records_or_facts() {
    support::run_async_test("claim-snapshot-independent", || async {
        let origin = support::engine().await;
        let node = support::engine().await;
        let issuer = support::person(&origin, "snapshot.issuer").await;
        let reader = support::person(&node, "snapshot.reader").await;
        authorize(&origin, &issuer).await;
        authorize(&node, &reader).await;
        let review = claims::reviewed(
            &origin,
            &issuer,
            claims::draft(&issuer, true),
            "snapshot:preview",
        )
        .await;
        let saved = claims::publish(&origin, &issuer, &review, "snapshot:publish")
            .await
            .unwrap();
        let document: model::Document =
            serde_json::from_value(saved.data.unwrap()["trust"]["document"].clone()).unwrap();
        let before: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key)").fetch_one(&node.store.pool).await.unwrap();
        assert!(
            check(&node, &reader, &document, "snapshot:unknown")
                .await
                .is_err()
        );
        let after: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key)").fetch_one(&node.store.pool).await.unwrap();
        assert_eq!(before, after);
        recognize(&node, &issuer).await;
        let before: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&node.store.pool)
        .await
        .unwrap();
        let result = check(&node, &reader, &document, "snapshot:check")
            .await
            .unwrap();
        assert!(result.created.is_none());
        assert!(result.facts.is_empty());
        let result = result.data.unwrap();
        assert_eq!(result["trust"]["kind"], "claim_check");
        assert_eq!(result["trust"]["retained"], false);
        assert_eq!(result["trust"]["support"]["claimed"]["value"], "100");
        assert_eq!(result["trust"]["support"]["state"], "unknown");
        let after: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)",
        )
        .fetch_one(&node.store.pool)
        .await
        .unwrap();
        assert_eq!(before, after);
        let mut restricted = document.clone();
        restricted.statement.audience = Audience::Selected {
            viewers: vec![Identity::Person(issuer.uid.clone())],
        };
        restricted.signature.clear();
        engine::trust_analysis::sign_claim_document(&mut restricted, &issuer.signer).unwrap();
        assert!(
            check(&node, &reader, &restricted, "snapshot:restricted")
                .await
                .is_err()
        );
        let mut forged = document.clone();
        forged.statement.amount = nucleus::DecimalValue::parse_inferred("1000").unwrap();
        assert!(
            check(&node, &reader, &forged, "snapshot:forged")
                .await
                .is_err()
        );
        store::sqlx::query("DELETE FROM identity_key WHERE actor_uid=?")
            .bind(&issuer.uid)
            .execute(&node.store.pool)
            .await
            .unwrap();
        assert!(
            check(&node, &reader, &document, "snapshot:revoked")
                .await
                .is_err()
        );
    });
}

#[test]
fn snapshots_cannot_bypass_a_readers_retained_hidden_withdrawal_or_same_revision_fork() {
    support::run_async_test("claim-snapshot-control", || async {
        let node = support::engine().await;
        let issuer = support::person(&node, "snapshot.controller").await;
        authorize(&node, &issuer).await;
        let review = claims::reviewed(
            &node,
            &issuer,
            claims::draft(&issuer, true),
            "snapshot:control-preview",
        )
        .await;
        let saved = claims::publish(&node, &issuer, &review, "snapshot:control-publish")
            .await
            .unwrap();
        let original: model::Document =
            serde_json::from_value(saved.data.unwrap()["trust"]["document"].clone()).unwrap();
        assert!(
            check(&node, &issuer, &original, "snapshot:original")
                .await
                .is_ok()
        );
        let mut fork = original.clone();
        fork.statement.amount = nucleus::DecimalValue::parse_inferred("101").unwrap();
        fork.signature.clear();
        engine::trust_analysis::sign_claim_document(&mut fork, &issuer.signer).unwrap();
        assert!(
            check(&node, &issuer, &fork, "snapshot:unretained-fork")
                .await
                .is_err()
        );
        let mut withdrawal = claims::draft(&issuer, true);
        withdrawal.claim = Some(original.statement.claim.clone());
        withdrawal.descriptor = original.statement.descriptor.clone();
        withdrawal.expected_revision = 1;
        withdrawal.amount = "0".into();
        withdrawal.withdrawn = true;
        let review =
            claims::reviewed(&node, &issuer, withdrawal, "snapshot:withdraw-preview").await;
        let saved = claims::publish(&node, &issuer, &review, "snapshot:withdraw")
            .await
            .unwrap();
        store::sqlx::query("UPDATE record SET deleted_at='2026-10-17T00:00:00Z' WHERE uid=?")
            .bind(saved.created.unwrap())
            .execute(&node.store.pool)
            .await
            .unwrap();
        assert!(
            check(&node, &issuer, &original, "snapshot:hidden-withdrawal")
                .await
                .is_err()
        );
        let inspected = node
            .act(
                command(
                    &issuer,
                    "snapshot:inspect-old",
                    ClaimCommand::InspectDocument {
                        document: serde_json::to_string(&original).unwrap(),
                        as_organ: false,
                    },
                ),
                Some(issuer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(inspected["trust"]["not_known_superseded"], false);
    });
}

#[test]
fn public_profile_claim_snapshots_preserve_originals_and_reject_private_foreign_duplicate_or_forged_data()
 {
    support::run_async_test("profile-claim-publication", || async {
        let node = support::engine().await;
        let person = support::person(&node, "profile.claim-editor").await;
        authorize(&node, &person).await;
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owner-key");
        std::fs::write(&path, [117; 32]).unwrap();
        node.set_root_key_path(path);
        let mut draft = claims::draft(&person, true);
        draft.descriptor.subject = Identity::Organ(organ.uid.clone());
        let review = claims::reviewed(&node, &person, draft, "profile:snapshot-review").await;
        let saved = claims::publish(&node, &person, &review, "profile:snapshot-sign")
            .await
            .unwrap();
        let original: model::Document =
            serde_json::from_value(saved.data.unwrap()["trust"]["document"].clone()).unwrap();
        let fields = nucleus::social::ProfileFields {
            name: "Food cooperative".into(),
            claims: vec![original.clone()],
            ..Default::default()
        };
        let host = support::engine().await;
        host.act(
            Action::Social {
                request: nucleus::social::Command::ConfigureServices {
                    settings: nucleus::social::ServiceSettings {
                        directory: true,
                        ..Default::default()
                    },
                },
            },
            None,
        )
        .await
        .unwrap();
        let endpoint = iroh::SecretKey::from_bytes(&[118; 32]).public().to_string();
        let peer = iroh::SecretKey::from_bytes(&[119; 32]).public().to_string();
        let saved = node
            .act(
                Action::Social {
                    request: nucleus::social::Command::SaveProfile {
                        fields: fields.clone(),
                        parents: vec![],
                        destinations: vec![endpoint.clone()],
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let profile: nucleus::social::Profile =
            serde_json::from_value(saved["profile"].clone()).unwrap();
        assert_eq!(profile.fields.claims, vec![original.clone()]);
        host.social_public_request(
            &peer,
            &endpoint,
            nucleus::social::PublicRequest::PublishProfile { document: profile },
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
        let hosted = host
            .social_public_request(
                &peer,
                &endpoint,
                nucleus::social::PublicRequest::FetchProfile {
                    organ: organ.uid.clone(),
                },
                nucleus::execution::now().timestamp(),
            )
            .await
            .unwrap();
        assert!(
            serde_json::to_string(&hosted)
                .unwrap()
                .contains(&original.signature)
        );
        let keys: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_key WHERE actor_uid=?")
                .bind(&person.uid)
                .fetch_one(&host.store.pool)
                .await
                .unwrap();
        assert_eq!(keys, 0);
        let parents = vec![saved["hash"].as_str().unwrap().to_owned()];
        let mut restricted = original.clone();
        restricted.statement.audience = Audience::default();
        restricted.signature.clear();
        engine::trust_analysis::sign_claim_document(&mut restricted, &person.signer).unwrap();
        let mut foreign = original.clone();
        foreign.statement.descriptor.subject = Identity::Person(person.uid.clone());
        foreign.signature.clear();
        engine::trust_analysis::sign_claim_document(&mut foreign, &person.signer).unwrap();
        let mut forged = original.clone();
        forged.statement.amount = nucleus::DecimalValue::parse_inferred("9999").unwrap();
        for claims in [
            vec![restricted],
            vec![foreign],
            vec![forged],
            vec![original.clone(), original.clone()],
            vec![original.clone(); 5],
        ] {
            let mut rejected = fields.clone();
            rejected.claims = claims;
            assert!(
                node.act(
                    Action::Social {
                        request: nucleus::social::Command::SaveProfile {
                            fields: rejected,
                            parents: parents.clone(),
                            destinations: vec![]
                        }
                    },
                    None
                )
                .await
                .is_err()
            );
        }
        let state = store::records::get_extension(
            &node.store.pool,
            &organ.uid,
            nucleus::social::PROFILE_NAMESPACE,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            state["published"]["fields"]["claims"],
            serde_json::to_value(vec![original]).unwrap()
        );
    });
}
