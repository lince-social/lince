use super::*;
use nucleus::trust_analysis::{
    claims::{
        self, Comparator, Descriptor, Metric,
        commands::{Command as ClaimCommand, Draft},
    },
    query::{CounterpartyBasis, EvidenceLevel, Query},
};

fn descriptor(person: &str) -> Descriptor {
    Descriptor {
        subject: Identity::Person(person.into()),
        role: nucleus::transfer::outcome::Role::Giver,
        measurement: nucleus::transfer::receipt::Measurement {
            meaning: nucleus::vocabulary::reference::MeaningReference {
                vocabulary_uid: nucleus::new_uid("g"),
                concept_uid: nucleus::new_uid("c"),
                revision: 1,
                definition_hash: "a".repeat(64),
            },
            unit: None,
            object_commitment: None,
        },
        level: EvidenceLevel::NeedLinkedReceiver,
        metric: Metric::Quantity,
        counterparty_basis: CounterpartyBasis::Organ,
        from: None,
        until: None,
    }
}

pub(super) fn draft(person: &support::Person, public: bool) -> Draft {
    Draft {
        issuer: Identity::Person(person.uid.clone()),
        claim: None,
        title: "Food contributions".into(),
        descriptor: descriptor(&person.uid),
        comparator: Comparator::AtLeast,
        amount: "100".into(),
        as_of: None,
        lifetime_seconds: 3600,
        expected_revision: 0,
        withdrawn: false,
        audience: if public {
            Audience::Public
        } else {
            Audience::default()
        },
        key_id: String::new(),
        public_key: String::new(),
    }
}

fn command(person: &support::Person, id: &str, request: ClaimCommand) -> Action {
    action(person, id, Command::Claims { request })
}

pub(super) async fn reviewed(
    node: &Engine,
    person: &support::Person,
    draft: Draft,
    id: &str,
) -> Value {
    node.set_signer(person.signer.clone()).await.unwrap();
    let mut action = command(person, id, ClaimCommand::Preview { draft });
    engine::trust_analysis::prepare_contact_action(&mut action, &person.signer).unwrap();
    node.act(action, Some(person.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap()["trust"]
        .clone()
}

pub(super) async fn publish(
    node: &Engine,
    person: &support::Person,
    review: &Value,
    id: &str,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    let mut action = command(
        person,
        id,
        ClaimCommand::Publish {
            review: serde_json::from_value(review["review"].clone()).unwrap(),
            expected_review: serde_json::from_value(review["hash"].clone()).unwrap(),
        },
    );
    engine::trust_analysis::prepare_contact_action(&mut action, &person.signer).unwrap();
    node.act(action, Some(person.uid.clone())).await
}

#[test]
fn claims_have_original_signatures_private_holders_and_reader_owned_unknown_support() {
    support::run_async_test("profile-claim-originality", || async {
        let producer = support::engine().await;
        let reader = support::engine().await;
        let issuer = support::person(&producer, "claim.issuer").await;
        let viewer = support::person(&reader, "claim.viewer").await;
        authorize(&producer, &issuer).await;
        authorize(&reader, &viewer).await;
        recognize(&reader, &issuer).await;
        let review = reviewed(&producer, &issuer, draft(&issuer, true), "claim:preview").await;
        let first = publish(&producer, &issuer, &review, "claim:publish")
            .await
            .unwrap();
        assert_eq!(first.facts.len(), 1);
        let record = first.created.unwrap();
        assert!(
            publish(&producer, &issuer, &review, "claim:publish")
                .await
                .unwrap()
                .facts
                .is_empty()
        );
        let exported = producer
            .act(
                command(
                    &issuer,
                    "claim:export",
                    ClaimCommand::Export {
                        record: record.clone(),
                        as_organ: false,
                    },
                ),
                Some(issuer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]["export"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!exported.contains(&record));
        let proof = claims::Document::parse(exported.as_bytes()).unwrap();
        assert_eq!(
            reader
                .authenticate_profile_claim(&proof, nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            engine::trust_analysis::IdentityAuthenticity::Accepted
        );
        let imported = reader
            .act(
                command(
                    &viewer,
                    "claim:import",
                    ClaimCommand::Import {
                        document: exported,
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut sources = Query::for_subject(Identity::Person(viewer.uid.clone()));
        sources.rely_on_all_visible = true;
        let check = reader
            .act(
                command(
                    &viewer,
                    "claim:check",
                    ClaimCommand::Check {
                        record: imported.clone(),
                        sources,
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap();
        assert!(check.facts.is_empty());
        assert_eq!(check.data.unwrap()["trust"]["support"]["state"], "unknown");
        let other = support::person(&reader, "claim.other").await;
        authorize(&reader, &other).await;
        assert!(
            reader
                .act(
                    command(
                        &other,
                        "claim:denied",
                        ClaimCommand::Inspect {
                            record: imported,
                            as_organ: false
                        }
                    ),
                    Some(other.uid.clone())
                )
                .await
                .is_err()
        );
        let mut forged = proof.clone();
        forged.signature = "A".repeat(86) + "==";
        assert!(
            reader
                .authenticate_profile_claim(&forged, nucleus::execution::now().timestamp())
                .await
                .is_err()
        );
        let transfer_count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
                .fetch_one(&reader.store.pool)
                .await
                .unwrap();
        assert_eq!(transfer_count, 0);
    });
}

#[test]
fn claim_reviews_reject_edits_stale_profiles_private_audience_bypass_and_instruction_imports() {
    support::run_async_test("profile-claim-review-checkpoints", || async {
        let node = support::engine().await;
        let person = support::person(&node, "claim.owner").await;
        let other = support::person(&node, "claim.outsider").await;
        authorize(&node, &person).await;
        authorize(&node, &other).await;
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let edit_self = store::auth::ensure_permission(&node.store.pool, "user", "update_self")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, edit_self)
            .await
            .unwrap();
        let preview = reviewed(&node, &person, draft(&person, false), "claim:preview").await;
        let mut edited = preview.clone();
        edited["review"]["document"]["statement"]["amount"] = json!({"scale":0,"value":"1000"});
        assert!(
            publish(&node, &person, &edited, "claim:changed")
                .await
                .is_err()
        );
        node.act(
            Action::EditRecordText {
                target: person.uid.clone(),
                head: Some("Changed profile".into()),
                body: None,
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
        assert!(
            publish(&node, &person, &preview, "claim:stale")
                .await
                .is_err()
        );
        let preview = reviewed(&node, &person, draft(&person, false), "claim:fresh").await;
        let created = publish(&node, &person, &preview, "claim:private")
            .await
            .unwrap()
            .created
            .unwrap();
        let doc =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &created)
                .await
                .unwrap()
                .document;
        assert!(
            node.act(
                command(
                    &other,
                    "claim:private-bypass",
                    ClaimCommand::Import {
                        document: serde_json::to_string(&doc).unwrap(),
                        as_organ: false
                    }
                ),
                Some(other.uid.clone())
            )
            .await
            .is_err()
        );
        assert!(
            node.act(
                command(
                    &person,
                    "claim:instruction",
                    ClaimCommand::Import {
                        document: "{\"kind\":\"delete_record\"}".into(),
                        as_organ: false
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn claim_preview_requires_current_recognized_keys_and_signatures_are_purpose_bound() {
    support::run_async_test("profile-claim-signing-authority", || async {
        let node = support::engine().await;
        let person = support::person(&node, "claim.signer").await;
        authorize(&node, &person).await;
        let stranger = engine::trust::Signer::generate(&person.uid, "unrecognized-claim-key");
        let mut unknown = draft(&person, true);
        unknown.key_id = stranger.key_id.clone();
        unknown.public_key = stranger.public_key_b64();
        assert!(
            node.act(
                command(
                    &person,
                    "claim:unknown-key",
                    ClaimCommand::Preview { draft: unknown }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let preview = reviewed(&node, &person, draft(&person, true), "claim:recognized-key").await;
        let mut proof: claims::Document =
            serde_json::from_value(preview["review"]["document"].clone()).unwrap();
        proof.signature = person
            .signer
            .sign_bytes(b"lince.contact-disclosure.signature.v1");
        assert!(
            node.authenticate_profile_claim(&proof, nucleus::execution::now().timestamp())
                .await
                .is_err()
        );
        engine::trust_analysis::sign_claim_document(&mut proof, &stranger).unwrap_err();
        let (key, signature) = node.revocation_certificate(&person.signer);
        node.adopt_revocation(&person.uid, &key, &signature)
            .await
            .unwrap();
        assert!(
            publish(&node, &person, &preview, "claim:revoked-after-preview")
                .await
                .is_err()
        );
        let mut expired = draft(&person, true);
        expired.key_id = person.signer.key_id.clone();
        expired.public_key = person.signer.public_key_b64();
        assert!(
            node.act(
                command(
                    &person,
                    "claim:revoked-preview",
                    ClaimCommand::Preview { draft: expired }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn organ_claims_require_administration_owner_authority_and_explicit_organ_reading() {
    support::run_async_test("profile-claim-organ-authority", || async {
        let node = support::engine().await;
        let person = support::person(&node, "claim.organ-admin").await;
        authorize(&node, &person).await;
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("root-key");
        let root =
            engine::trust::Signer::load_or_create(&path, &organ.uid, engine::roster::ROOT_KEY_ID)
                .unwrap();
        node.set_root_key_path(path.clone());
        node.publish_root_key(&root).await.unwrap();
        let mut organization = draft(&person, false);
        organization.issuer = Identity::Organ(organ.uid.clone());
        organization.descriptor.subject = Identity::Organ(organ.uid.clone());
        assert!(
            node.act(
                command(
                    &person,
                    "claim:organ-denied",
                    ClaimCommand::Preview {
                        draft: organization.clone()
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let review = node
            .act(
                command(
                    &person,
                    "claim:organ-preview",
                    ClaimCommand::Preview {
                        draft: organization,
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        let publish = command(
            &person,
            "claim:organ-publish",
            ClaimCommand::Publish {
                review: serde_json::from_value(review["review"].clone()).unwrap(),
                expected_review: serde_json::from_value(review["hash"].clone()).unwrap(),
            },
        );
        let result = node.act(publish.clone(), None).await.unwrap();
        let record = result.created.unwrap();
        assert!(node.act(publish, None).await.unwrap().facts.is_empty());
        assert!(
            node.act(
                command(
                    &person,
                    "claim:organ-person-read",
                    ClaimCommand::Inspect {
                        record: record.clone(),
                        as_organ: false
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let inspected = node
            .act(
                command(
                    &person,
                    "claim:organ-read",
                    ClaimCommand::Inspect {
                        record: record.clone(),
                        as_organ: true,
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(inspected["trust"]["authority_current"], true);
        let mut proof: claims::Document =
            serde_json::from_value(inspected["trust"]["document"].clone()).unwrap();
        let cell_key = engine::trust::Signer::generate(&organ.uid, "cell:receipt-only");
        engine::trust::adopt_key(
            &node.store,
            &organ.uid,
            &cell_key.key_id,
            &cell_key.public_key_b64(),
        )
        .await
        .unwrap();
        proof.key_id = cell_key.key_id.clone();
        proof.public_key = cell_key.public_key_b64();
        proof.signature = cell_key.sign_bytes(&proof.signing_bytes().unwrap());
        assert!(
            node.authenticate_profile_claim(&proof, nucleus::execution::now().timestamp())
                .await
                .is_err()
        );
    });
}

#[test]
fn hidden_withdrawal_and_signed_forks_withhold_old_claims_without_resurrection() {
    support::run_async_test("profile-claim-hidden-controls", || async {
        let node = support::engine().await;
        let person = support::person(&node, "claim.revisions").await;
        authorize(&node, &person).await;
        let preview = reviewed(&node, &person, draft(&person, true), "claim:first-review").await;
        let first = publish(&node, &person, &preview, "claim:first")
            .await
            .unwrap()
            .created
            .unwrap();
        let original =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &first)
                .await
                .unwrap()
                .document;
        let mut withdrawal = draft(&person, true);
        withdrawal.claim = Some(original.statement.claim.clone());
        withdrawal.expected_revision = 1;
        withdrawal.withdrawn = true;
        withdrawal.amount = "0".into();
        let preview = reviewed(&node, &person, withdrawal, "claim:withdraw-review").await;
        let withdrawn = publish(&node, &person, &preview, "claim:withdraw")
            .await
            .unwrap()
            .created
            .unwrap();
        store::sqlx::query("UPDATE record SET deleted_at=? WHERE uid=?")
            .bind(nucleus::execution::now().to_rfc3339())
            .bind(&withdrawn)
            .execute(&node.store.pool)
            .await
            .unwrap();
        let inspected = node
            .act(
                command(
                    &person,
                    "claim:older",
                    ClaimCommand::Inspect {
                        record: first.clone(),
                        as_organ: false,
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(inspected["trust"]["current_retained"], false);
        let serialized = inspected.to_string();
        assert!(!serialized.contains(&withdrawn));
        assert!(
            node.act(
                command(
                    &person,
                    "claim:older-check",
                    ClaimCommand::Check {
                        record: first.clone(),
                        sources: Query::for_subject(Identity::Person(person.uid.clone())),
                        as_organ: false
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let replay = node
            .act(
                command(
                    &person,
                    "claim:older-import",
                    ClaimCommand::Import {
                        document: serde_json::to_string(&original).unwrap(),
                        as_organ: false,
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap();
        assert!(replay.facts.is_empty());
        assert_eq!(replay.created.as_deref(), Some(first.as_str()));
        let mut fork = original;
        fork.statement.revision = 2;
        fork.statement.amount = nucleus::DecimalValue::parse_inferred("200").unwrap();
        fork.signature.clear();
        engine::trust_analysis::sign_claim_document(&mut fork, &person.signer).unwrap();
        let retained = node
            .act(
                command(
                    &person,
                    "claim:fork",
                    ClaimCommand::Import {
                        document: serde_json::to_string(&fork).unwrap(),
                        as_organ: false,
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let forked = node
            .act(
                command(
                    &person,
                    "claim:fork-inspect",
                    ClaimCommand::Inspect {
                        record: retained,
                        as_organ: false,
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(forked["trust"]["current_retained"], false);
        let listed = node
            .act(
                command(
                    &person,
                    "claim:list-revisions",
                    ClaimCommand::List {
                        subject: Identity::Person(person.uid.clone()),
                        before: None,
                        as_organ: false,
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let entries = listed["trust"]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["current_retained"], false);
        assert!(!listed.to_string().contains(&withdrawn));
    });
}
