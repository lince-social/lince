use super::*;
use nucleus::transfer::{public_history::Terms, receipt::{self, ApprovalRole, Document, ScopeRequest, Stage}};
use nucleus::trust_analysis::{Identity, contacts::Audience, serving::Artifact};

fn scope_request(fixture: &Fixture) -> ScopeRequest {
    ScopeRequest {
        audience: Audience::Public,
        use_upfront_terms: true,
        public_person_roles: Vec::new(),
        public_measurements: Vec::new(),
        occurrence: fixture.occurrence.clone(),
        expected_revision: fixture.transfer.revision,
        outcome_records: Vec::new(),
        stages: vec![Stage::Settled, Stage::Dispute],
        publish_agreed_quantity: false,
        expires_at: nucleus::execution::now().timestamp() + i64::from(nucleus::transfer::public_history::DEFAULT_RETENTION_SECONDS),
    }
}

async fn act(fixture: &Fixture, id: &str, request: receipt::Command) -> engine::actions::ActionOutcome {
    fixture.engine.act(Action::TransferReceipt { person:Some(fixture.giver.uid.clone()), request_id:id.into(), request }, None).await.unwrap()
}

#[test]
fn upfront_history_is_signed_before_agreement_and_never_replaces_origin_attestation() {
    support::run_async_test("upfront-public-history", || async {
        let terms = Terms::default();
        let fixture = Fixture::configured_with_history(false, 3, None, None, Some(terms.clone())).await;
        let original = store::transfers::revision_fact(&fixture.engine.store.pool, &fixture.transfer.transfer, fixture.transfer.revision).await.unwrap().unwrap();
        let revision: nucleus::transfer::TransferRevisionEvidence = serde_json::from_str(original.payload.as_deref().unwrap()).unwrap();
        assert_eq!(revision.terms.transfer.public_history.as_ref(), Some(&terms));
        let before:i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact").fetch_one(&fixture.engine.store.pool).await.unwrap();
        let changed = fixture.engine.act(Action::SetExtension { target:fixture.transfer.transfer.clone(), namespace:nucleus::transfer::public_history::NAMESPACE.into(), fds:json!({"terms":null}) }, None).await;
        assert!(changed.is_err());
        let scope = fixture.engine.prepare_transfer_receipt_scope(scope_request(&fixture), None, nucleus::execution::now()).await.unwrap();
        assert_eq!(scope.upfront_terms.as_ref(), Some(&terms));
        assert!(scope.expires_at <= scope.source.activated_at + i64::from(terms.retention_seconds));
        assert_eq!(scope.upfront_roles(), std::collections::BTreeSet::from([ApprovalRole::Giver, ApprovalRole::Receiver]));
        assert_eq!(before, store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM fact").fetch_one(&fixture.engine.store.pool).await.unwrap());
        for original in &scope.original_authorities {
            let encoded = serde_json::to_string(original.authority.participation.as_ref().unwrap()).unwrap();
            assert!(!encoded.contains(&terms.nonce));
            assert!(encoded.contains(terms.id().unwrap().as_str()));
        }
        let prepared = act(&fixture, "history:prepare", receipt::Command::Prepare { scope:scope_request(&fixture) }).await.created.unwrap();
        let saved = store::transfer_receipts::get(&fixture.engine.store.pool, &prepared).await.unwrap();
        let Document::Review(review) = saved.document else { panic!("Expected a reviewed scope") };
        assert!(!review.is_complete());
        assert!(review.approvals.is_empty());
        let owner = fixture.authorizer.as_ref().unwrap();
        let authority = fixture.engine.create_transfer_receipt_authority(&owner.root, &owner.cell, nucleus::execution::now()).await.unwrap();
        let key = act(&fixture, "history:publication-key", receipt::Command::Import { document:serde_json::to_string(&Document::Authority(authority)).unwrap() }).await.created.unwrap();
        act(&fixture, "history:origin-attests", receipt::Command::Approve { record:prepared.clone(), expected_version:saved.version, role:ApprovalRole::Origin, authority_record:key, private_transfer:None }).await;
        let saved = store::transfer_receipts::get(&fixture.engine.store.pool, &prepared).await.unwrap();
        let Document::Review(review) = saved.document else { panic!("Expected an approved scope") };
        assert!(review.is_complete());
        assert_eq!(review.approvals.len(), 1);
        assert_eq!(review.approvals[0].role, ApprovalRole::Origin);
        fixture.engine.verify_transfer_receipt_review(&review, nucleus::execution::now()).await.unwrap();
        let mut expanded = scope_request(&fixture);
        expanded.publish_agreed_quantity = true;
        assert!(fixture.engine.prepare_transfer_receipt_scope(expanded.clone(), None, nucleus::execution::now()).await.is_err());
        expanded.use_upfront_terms = false;
        let new_scope = fixture.engine.prepare_transfer_receipt_scope(expanded, None, nucleus::execution::now()).await.unwrap();
        assert!(new_scope.upfront_terms.is_none());
        assert!(new_scope.upfront_roles().is_empty());
        let mut stale = review.clone();
        stale.scope = new_scope;
        assert!(!stale.is_complete());
        assert!(fixture.engine.verify_transfer_receipt_review(&stale, nucleus::execution::now()).await.is_err());
        let outsider = Identity::Person(fixture.stranger.uid.clone());
        let mut restricted = review.as_bundle();
        restricted.publication.scope.upfront_terms = None;
        restricted.publication.scope.audience = Audience::Selected { viewers:vec![Identity::Person(fixture.receiver.uid.clone())] };
        let artifact = Artifact::Receipt(Box::new(Document::Publication(restricted)));
        assert!(artifact.allows_reader(&Identity::Person(fixture.receiver.uid.clone())));
        assert!(!artifact.allows_reader(&outsider));
        assert!(!artifact.allows_reader(&Identity::Organ(nucleus::new_uid("r"))));
        let expires = scope.source.activated_at + i64::from(terms.retention_seconds);
        let future = chrono::DateTime::from_timestamp(expires + 1, 0).unwrap();
        let mut renewal = scope_request(&fixture);
        renewal.expires_at = expires + 86400;
        assert!(fixture.engine.prepare_transfer_receipt_scope(renewal.clone(), None, future).await.is_err());
        renewal.use_upfront_terms = false;
        let renewal = fixture.engine.prepare_transfer_receipt_scope(renewal, None, future).await.unwrap();
        assert!(renewal.upfront_roles().is_empty());
        assert_ne!(renewal.id().unwrap(), scope.id().unwrap());
    });
}

#[test]
fn changing_public_history_terms_advances_signed_revision_and_rejects_old_participation() {
    support::run_async_test("public-history-revision-binding", || async {
        for variant in 0..3 {
        let node = support::engine().await;
        let owner = Authorizer::new(&node, 105).await;
        let giver = support::person(&node, "history.giver").await;
        let receiver = support::person(&node, "history.receiver").await;
        let stock = support::plain(&node, "history.stock", 10.0).await;
        let promise = support::promise("history-offer", &stock, &giver, -1.0);
        let terms = Terms::default();
        let transfer = support::create_transfer_with_history(&node, &giver, std::slice::from_ref(&receiver), vec![promise.clone()], DraftOptions { slug:"history.proposal", agreement:nucleus::transfer::AgreementType::Full, reserve_default:engine::actions::TransferReservePoint::Active, require_confirmation:true }, Some(terms.clone())).await;
        let original = node.prepare_transfer_participation(&transfer.transfer, transfer.revision, None, None).await.unwrap();
        assert_eq!(original.public_history_terms.as_ref(), Some(&terms));
        let authority = node.create_transfer_participation_authority(&owner.root, &owner.cell, original.terms, nucleus::execution::now()).await.unwrap();
        node.set_signer(giver.signer.clone()).await.unwrap();
        let old = node.act(Action::TransferReceipt { person:Some(giver.uid.clone()), request_id:"history:old-original-grant".into(), request:receipt::Command::Import { document:serde_json::to_string(&Document::Authority(authority)).unwrap() } }, None).await.unwrap().created.unwrap();
        let mut revised = terms.clone();
        match variant {
            0 => revised.retention_seconds = 86400,
            1 => revised.agreed_quantity = true,
            _ => revised.audience = Audience::Selected {viewers:vec![Identity::Person(receiver.uid.clone())]},
        }
        assert_ne!(revised.id().unwrap(),terms.id().unwrap());
        let stored = store::transfers::promises_of(&node.store.pool, &transfer.transfer).await.unwrap();
        let mut promise = promise;
        promise.uid = Some(stored[0].uid.clone());
        node.act(Action::ReviseTransferDraft {
            transfer:transfer.transfer.clone(), expected_revision:transfer.revision, request_id:"history:revise-public-term".into(),
            draft:engine::actions::TransferDraftRevisionInput { public_history:Some(revised.clone()), creator:giver.uid.clone(), slug:Some("history.proposal".into()), head:"history.proposal".into(), agreement:nucleus::transfer::AgreementType::Full, agreement_pct:None, satiation:engine::actions::TransferSatiation::None, parent:None, source:None, visibility:engine::actions::TransferVisibility::Hidden, max_proximity:None, reserve_default:engine::actions::TransferReservePoint::Active, require_confirmation:true, default_place:None, invitees:Vec::new(), promises:vec![promise], dependencies:Vec::new() },
        }, None).await.unwrap();
        let revision = support::current_revision(&node, &transfer.transfer).await;
        assert!(revision > transfer.revision);
        let fact = store::transfers::revision_fact(&node.store.pool, &transfer.transfer, revision).await.unwrap().unwrap();
        let frozen:nucleus::transfer::TransferRevisionEvidence = serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
        assert_eq!(frozen.terms.transfer.public_history, Some(revised));
        let updated = TransferFixture { transfer:transfer.transfer.clone(), revision };
        support::agree(&node, &updated, &giver, "history:agree-new:giver").await;
        support::agree(&node, &updated, &receiver, "history:agree-new:receiver").await;
        node.set_signer(giver.signer.clone()).await.unwrap();
        let denied = node.act(Action::ActivateTransferOccurrence { transfer:transfer.transfer, promise:stored[0].uid.clone(), expected_revision:revision, request_id:"history:stale-original-activation".into(), person:Some(giver.uid), original_authorities:vec![old] }, None).await;
        assert!(denied.is_err());
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM transfer_occurrence").fetch_one(&node.store.pool).await.unwrap(), 0);
        }
    });
}
