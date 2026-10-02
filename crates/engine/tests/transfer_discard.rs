use engine::actions::{Action, TransferReservePoint};
use nucleus::transfer::AgreementType;
mod support;

#[tokio::test]
async fn only_unused_creator_drafts_can_be_discarded_with_signed_retryable_evidence() {
    let engine = support::engine().await;
    let creator = support::person(&engine, "creator").await;
    let other = support::person(&engine, "other").await;
    let draft = support::create_transfer(
        &engine,
        &creator,
        &[],
        vec![],
        support::DraftOptions {
            slug: "unused",
            agreement: AgreementType::Individual,
            reserve_default: TransferReservePoint::None,
            require_confirmation: false,
        },
    )
    .await;
    let request = Action::DiscardTransferDraft {
        transfer: draft.transfer.clone(),
        person: Some(creator.uid.clone()),
        expected_revision: draft.revision,
        request_id: "discard-unused".into(),
    };
    assert!(
        engine
            .act(
                Action::DiscardTransferDraft {
                    transfer: draft.transfer.clone(),
                    person: Some(other.uid.clone()),
                    expected_revision: draft.revision,
                    request_id: "wrong-person".into()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::DiscardTransferDraft {
                    transfer: draft.transfer.clone(),
                    person: Some(creator.uid.clone()),
                    expected_revision: draft.revision + 1,
                    request_id: "stale".into()
                },
                None
            )
            .await
            .is_err()
    );
    let result = engine.act(request.clone(), None).await.unwrap();
    assert!(result.facts[0].signature.is_some());
    assert!(
        store::records::get(&engine.store.pool, &draft.transfer)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        engine.act(request, None).await.unwrap().data.unwrap()["duplicate"],
        true
    );
    let asset = support::plain(&engine, "asset", 10.0).await;
    let promise = support::promise(&nucleus::new_uid("promise"), &asset, &creator, -1.0);
    let addressed = support::create_transfer(
        &engine,
        &creator,
        &[other],
        vec![promise],
        support::DraftOptions {
            slug: "addressed",
            agreement: AgreementType::Individual,
            reserve_default: TransferReservePoint::None,
            require_confirmation: false,
        },
    )
    .await;
    engine.set_signer(creator.signer.clone()).await.unwrap();
    let refusal = engine
        .act(
            Action::DiscardTransferDraft {
                transfer: addressed.transfer.clone(),
                person: Some(creator.uid),
                expected_revision: addressed.revision,
                request_id: "addressed-discard".into(),
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        refusal,
        engine::EngineError::Conflict {
            code: "transfer_draft_not_discardable",
            ..
        }
    ));
    assert!(
        store::records::get(&engine.store.pool, &addressed.transfer)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_dependency_on_a_draft_promise_prevents_discard() {
    let engine = support::engine().await;
    let creator = support::person(&engine, "promise-creator").await;
    let asset = support::plain(&engine, "promise-asset", 10.0).await;
    let promise_uid = nucleus::new_uid("promise");
    let upstream = support::create_transfer(
        &engine,
        &creator,
        &[],
        vec![support::promise(&promise_uid, &asset, &creator, -1.0)],
        support::DraftOptions {
            slug: "promise-upstream",
            agreement: AgreementType::Individual,
            reserve_default: TransferReservePoint::None,
            require_confirmation: false,
        },
    )
    .await;
    let dependent: Action = serde_json::from_value(serde_json::json!({
        "action": "create-transfer-draft",
        "request_id": "create-promise-dependent",
        "creator": creator.uid,
        "head": "Dependent on draft promise",
        "agreement": "dependency",
        "reserve_default": "none",
        "dependencies": [{
            "scope": "transfer",
            "upstream_kind": "promise",
            "upstream": promise_uid,
            "required_state": "agreed"
        }]
    }))
    .unwrap();
    engine.act(dependent, None).await.unwrap();
    let refusal = engine
        .act(
            Action::DiscardTransferDraft {
                transfer: upstream.transfer.clone(),
                person: Some(creator.uid),
                expected_revision: upstream.revision,
                request_id: "discard-referenced-promise".into(),
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        refusal,
        engine::EngineError::Conflict {
            code: "transfer_draft_not_discardable",
            ..
        }
    ));
    assert!(
        store::records::get(&engine.store.pool, &upstream.transfer)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn dependencies_inside_an_unused_draft_do_not_prevent_discard() {
    let engine = support::engine().await;
    let creator = support::person(&engine, "internal-creator").await;
    let asset = support::plain(&engine, "internal-asset", 10.0).await;
    let first = nucleus::new_uid("promise");
    let second = nucleus::new_uid("promise");
    engine.set_signer(creator.signer.clone()).await.unwrap();
    let draft: Action = serde_json::from_value(serde_json::json!({
        "action": "create-transfer-draft",
        "request_id": "create-internal-dependencies",
        "creator": creator.uid,
        "head": "Draft with internal dependencies",
        "agreement": "dependency",
        "reserve_default": "none",
        "promises": [
            support::promise(&first, &asset, &creator, -1.0),
            support::promise(&second, &asset, &creator, -2.0)
        ],
        "dependencies": [{
            "scope": "promise",
            "promise": second,
            "upstream_kind": "promise",
            "upstream": first,
            "required_state": "agreed"
        }]
    }))
    .unwrap();
    let transfer = engine.act(draft, None).await.unwrap().created.unwrap();
    let revision = support::current_revision(&engine, &transfer).await;
    let discarded = engine
        .act(
            Action::DiscardTransferDraft {
                transfer: transfer.clone(),
                person: Some(creator.uid),
                expected_revision: revision,
                request_id: "discard-internal-dependencies".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(discarded.facts[0].signature.is_some());
    assert!(
        store::records::get(&engine.store.pool, &transfer)
            .await
            .unwrap()
            .is_none()
    );
}
