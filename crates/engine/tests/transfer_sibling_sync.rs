mod support;

use engine::{
    Engine,
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    sync::OpBatch,
    trust::Signer,
};
use support::*;

async fn members() -> (Engine, Engine, String, Signer, Signer) {
    members_with_writers(true).await
}

async fn members_with_writers(both_write: bool) -> (Engine, Engine, String, Signer, Signer) {
    let source = engine().await;
    let target = engine().await;
    let organ = store::organs::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::generate(&organ, ROOT_KEY_ID);
    let mut members = Vec::new();
    let signer = source.operational_key_for(&organ).await.unwrap();
    let recipient_key = target.operational_key_for(&organ).await.unwrap();
    for (node, key, label) in [
        (&source, &signer, "source"),
        (&target, &recipient_key, "target"),
    ] {
        let cell = store::cells::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        let mut capabilities = full_capabilities();
        if !both_write && label == "target" {
            capabilities.retain(|capability| capability != engine::roster::CAP_WRITE);
        }
        members.push(CellEntry {
            cell_uid: cell.uid,
            node_id: label.into(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities,
        });
    }
    source.set_organ_signer(signer.clone()).await.unwrap();
    source.publish_root_key(&root).await.unwrap();
    let roster = source.publish_roster(&root, members).await.unwrap();
    target
        .join_organ(
            &EnrolmentInvite {
                node_id: "source".into(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "joined-for-test".into(),
                addrs: Vec::new(),
            },
            &roster,
            recipient_key,
        )
        .await
        .unwrap();
    (source, target, organ, signer, root)
}

#[test]
fn delayed_sibling_catchup_verifies_people_before_facts_and_includes_referenced_evidence() {
    run_async_test("delayed-sibling-catchup", || async {
        let (source, target, organ, _, _) = members_with_writers(false).await;
        let ana = person(&source, "catchup-ana").await;
        let beto = person(&source, "catchup-beto").await;
        let stock = plain(&source, "catchup-stock", 10.0).await;
        source.set_signer(ana.signer.clone()).await.unwrap();
        source
            .act(
                engine::actions::Action::SetRecordStockLimit {
                    record: stock.clone(),
                    person: ana.uid.clone(),
                    minimum: Some(store::exact::integer(5)),
                    expected_version: 0,
                    request_id: "catchup-stock-limit".into(),
                },
                None,
            )
            .await
            .unwrap();
        for _ in 0..96 {
            source
                .append(
                    nucleus::NewFact::quantity(
                        stock.clone(),
                        store::exact::zero(),
                        nucleus::Cause::user_edit(),
                    ),
                    chrono::Utc::now(),
                )
                .await
                .unwrap();
        }
        let fixture = create_transfer(
            &source,
            &ana,
            std::slice::from_ref(&beto),
            vec![
                promise("catchup-give", &stock, &ana, -2.0),
                promise("catchup-receive", &stock, &beto, 2.0),
            ],
            DraftOptions {
                slug: "catchup-transfer",
                agreement: nucleus::transfer::AgreementType::Full,
                reserve_default: engine::actions::TransferReservePoint::Agreed,
                require_confirmation: true,
            },
        )
        .await;
        let evidence = source
            .append(
                nucleus::NewFact::quantity(
                    fixture.transfer.clone(),
                    store::exact::zero(),
                    nucleus::Cause::user_edit(),
                ),
                chrono::Utc::now(),
            )
            .await
            .unwrap()
            .remove(0);
        let at = chrono::Utc::now().to_rfc3339();
        let mut tx = store::write_tx(&source.store.pool).await.unwrap();
        store::sqlx::query("INSERT INTO transfer_delivery_policy(uid,transfer_uid,origin_organ_uid,recipient_person_uid,recipient_organ_uid,mode,state,revision,created_at,updated_at) VALUES ('catchup-policy',?,?,?,?, 'replicated','active',1,?,?)")
            .bind(&fixture.transfer).bind(&organ).bind(&beto.uid).bind(&organ).bind(&at).bind(&at).execute(&mut *tx).await.unwrap();
        store::sqlx::query("INSERT INTO transfer_delivery_policy_event(uid,delivery_uid,revision,kind,to_mode,to_state,actor_person_uid,fact_uid,request_id,created_at) VALUES ('catchup-policy-event','catchup-policy',1,'created','replicated','active',?,?,'catchup-policy',?)")
            .bind(&ana.uid).bind(&evidence.uid).bind(&at).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let page = source.export_sync_page(&organ, &[], 2000).await.unwrap();
        let metadata: Vec<_> = page
            .batch
            .ops
            .iter()
            .filter(|op| op.tbl == "transfer_replication")
            .collect();
        assert!(!metadata.is_empty());
        assert_ne!(page.batch.ops[0].tbl, "transfer_replication");
        assert!(metadata.iter().any(|op| {
            let message: store::transfer_replication::Message =
                serde_json::from_str(op.value.as_deref().unwrap()).unwrap();
            message
                .transaction
                .changes
                .iter()
                .any(|change| change.table == "transfer_delivery_policy_event")
                && message
                    .transaction
                    .facts
                    .iter()
                    .any(|fact| fact.uid == evidence.uid && fact.hash == evidence.hash)
        }));
        target
            .receive_sync_batch(&organ, &page.batch)
            .await
            .unwrap();
        assert_eq!(
            store::organs::quarantine_count(&target.store.pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store::records::quantity(&target.store.pool, &stock)
                .await
                .unwrap()
                .unwrap(),
            store::exact::integer(10)
        );
        assert!(
            store::facts::get(&target.store.pool, &evidence.uid)
                .await
                .unwrap()
                .is_some()
        );
        let policies: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM transfer_delivery_policy WHERE transfer_uid = ?",
        )
        .bind(&fixture.transfer)
        .fetch_one(&target.store.pool)
        .await
        .unwrap();
        assert_eq!(policies, 1);
        assert_eq!(
            store::transfer_stock::get(&target.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .minimum,
            store::exact::integer(5)
        );
        assert_eq!(
            target
                .receive_sync_batch(&organ, &page.batch)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store::organs::quarantine_count(&target.store.pool)
                .await
                .unwrap(),
            0
        );
    });
}

#[test]
fn sibling_contact_bootstrap_is_local_until_an_explicit_policy_is_shared() {
    run_async_test("sibling-contact-bootstrap", || async {
        let (source, target, organ, _, _) = members().await;
        let foreign = "r_shared_foreign_contact";
        for node in [&source, &target] {
            store::organs::add_contact(&node.store.pool, foreign, None, "Foreign", "", 1)
                .await
                .unwrap();
            store::organs::set_trust(&node.store.pool, foreign, "known")
                .await
                .unwrap();
            assert!(
                store::transfer_replication::owner(&node.store.pool, "organ_contact", &[foreign])
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        store::organs::set_proximity(&source.store.pool, foreign, 3)
            .await
            .unwrap();
        store::organs::set_sync_policy(&source.store.pool, foreign, true, true)
            .await
            .unwrap();
        store::organs::set_mode(&source.store.pool, foreign, "hosted")
            .await
            .unwrap();
        store::organs::set_proximity(&target.store.pool, foreign, 2)
            .await
            .unwrap();
        store::organs::set_contact_scope(&source.store.pool, foreign, Some(&["head".into()]))
            .await
            .unwrap();
        store::organs::set_contact_accept_scope(
            &source.store.pool,
            foreign,
            Some(&["head".into()]),
        )
        .await
        .unwrap();
        store::organs::set_trust(&source.store.pool, foreign, "blocked")
            .await
            .unwrap();
        let page = source.export_sync_page(&organ, &[], 2000).await.unwrap();
        target
            .receive_sync_batch(&organ, &page.batch)
            .await
            .unwrap();
        let copied = store::organs::contact(&target.store.pool, foreign)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(copied.trust, "blocked");
        assert_eq!(copied.scope_fields, Some(vec!["head".into()]));
        assert_eq!(copied.accept_fields, Some(vec!["head".into()]));
        assert_eq!(copied.proximity, 2);
        assert_eq!(copied.mode, "replica");
        assert!(!copied.sync_out && !copied.sync_in);
        assert!(
            store::organs::set_trust(&target.store.pool, foreign, "known")
                .await
                .is_err()
        );
        assert!(
            store::organs::set_contact_scope(&target.store.pool, foreign, None)
                .await
                .is_err()
        );
        assert_eq!(
            store::organs::contact(&target.store.pool, foreign)
                .await
                .unwrap()
                .unwrap()
                .trust,
            "blocked"
        );
    });
}

#[test]
fn sibling_transactions_are_ordered_atomic_private_and_signed() {
    run_async_test("sibling-transfer-transactions", || async {
        let (source, target, organ, cell_signer, root) = members().await;
        let ana = person(&source, "ana").await;
        let beto = person(&source, "beto").await;
        let stock = plain(&source, "stock", 10.0).await;
        let fixture = create_transfer(
            &source,
            &ana,
            std::slice::from_ref(&beto),
            vec![
                promise("give", &stock, &ana, -2.0),
                promise("receive", &stock, &beto, 2.0),
            ],
            DraftOptions {
                slug: "replicated-transfer",
                agreement: nucleus::transfer::AgreementType::Full,
                reserve_default: engine::actions::TransferReservePoint::Agreed,
                require_confirmation: true,
            },
        )
        .await;
        agree(&source, &fixture, &ana, "ana-agreement").await;
        agree(&source, &fixture, &beto, "beto-agreement").await;
        let occurrence = activate(&source, &fixture, &ana, "give", "activate").await;
        claim(
            &source,
            &occurrence,
            &ana,
            engine::actions::TransferOccurrenceClaimRole::Delivery,
            "give",
        )
        .await;
        claim(
            &source,
            &occurrence,
            &beto,
            engine::actions::TransferOccurrenceClaimRole::Receipt,
            "receive",
        )
        .await;
        source.set_signer(ana.signer.clone()).await.unwrap();
        let preview = settlement_preview(&source, &occurrence, &ana, 2.0).await;
        settle_from_preview(&source, &occurrence, &ana, "settle", &preview).await;
        let foreign_organ = "r_foreign_organ";
        store::organs::add_contact(
            &source.store.pool,
            foreign_organ,
            None,
            "Foreign Organ",
            "",
            1,
        )
        .await
        .unwrap();
        store::organs::set_trust(&source.store.pool, foreign_organ, "known")
            .await
            .unwrap();
        store::organs::set_contact_scope(&source.store.pool, foreign_organ, Some(&["head".into()]))
            .await
            .unwrap();
        store::organs::set_contact_accept_scope(
            &source.store.pool,
            foreign_organ,
            Some(&["head".into()]),
        )
        .await
        .unwrap();
        store::visibility::set_hidden_from_organ(&source.store.pool, foreign_organ, &stock, true)
            .await
            .unwrap();
        store::sqlx::query("UPDATE organ_contact SET last_synced_seq = 999, peer_acked_seq = 777 WHERE record_uid = ?").bind(foreign_organ).execute(&source.store.pool).await.unwrap();
        let precise_amount = 0.30000000000000004;
        let precise = create_transfer(
            &source,
            &ana,
            std::slice::from_ref(&beto),
            vec![
                promise("precise-give", &stock, &ana, -precise_amount),
                promise("precise-receive", &stock, &beto, precise_amount),
            ],
            DraftOptions {
                slug: "precise-transfer",
                agreement: nucleus::transfer::AgreementType::Full,
                reserve_default: engine::actions::TransferReservePoint::None,
                require_confirmation: true,
            },
        )
        .await;
        let target_cell = store::cells::local(&target.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let refused = source
            .act(
                engine::actions::Action::DesignateTransferExecutor {
                    transfer_uid: fixture.transfer.clone(),
                    cell_uid: Some(target_cell),
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(refused.to_string().contains("writing Cell"));
        source
            .act(
                engine::actions::Action::DesignateTransferExecutor {
                    transfer_uid: fixture.transfer.clone(),
                    cell_uid: None,
                },
                None,
            )
            .await
            .unwrap();
        let page = source.export_sync_page(&organ, &[], 2000).await.unwrap();
        let (stream, ordinary): (Vec<_>, Vec<_>) = page
            .batch
            .ops
            .into_iter()
            .partition(|op| op.tbl == "transfer_replication");
        assert!(stream.len() > 3);
        target
            .receive_sync_batch(
                &organ,
                &OpBatch {
                    from_organ: organ.clone(),
                    ops: ordinary,
                },
            )
            .await
            .unwrap();
        let before = store::records::quantity(&target.store.pool, &stock)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.to_f64(), 10.0);
        let last = stream.last().unwrap().clone();
        let after_revocation = last.clone();
        let out_of_order = OpBatch {
            from_organ: organ.clone(),
            ops: vec![last],
        };
        assert_eq!(
            target
                .receive_sync_batch(&organ, &out_of_order)
                .await
                .unwrap(),
            0
        );
        assert!(!target.batch_is_saved(&out_of_order).await.unwrap());
        let foreign = engine().await;
        assert!(
            foreign
                .import_op_batch(&OpBatch {
                    from_organ: organ.clone(),
                    ops: stream.clone()
                })
                .await
                .is_err()
        );
        for op in stream {
            let mut message: store::transfer_replication::Message =
                serde_json::from_str(op.value.as_deref().unwrap()).unwrap();
            let mut forged = op.clone();
            let mut invalid_signature = message.clone();
            invalid_signature.signature = "invalid".into();
            forged.value = Some(serde_json::to_string(&invalid_signature).unwrap());
            assert!(
                target
                    .import_op_batch(&OpBatch {
                        from_organ: organ.clone(),
                        ops: vec![forged]
                    })
                    .await
                    .is_err()
            );
            if message
                .transaction
                .facts
                .iter()
                .any(|fact| !fact.delta.is_zero())
            {
                let quantity = store::records::quantity(&target.store.pool, &stock)
                    .await
                    .unwrap();
                message
                    .transaction
                    .changes
                    .push(store::transfer_replication::Change {
                        table: "configuration".into(),
                        before: None,
                        after: Some(serde_json::json!({"id":1})),
                    });
                message.signature = cell_signer.sign_bytes(
                    &store::transfer_replication::Message::signing_bytes(&message.transaction)
                        .unwrap(),
                );
                let mut bad = op.clone();
                bad.value = Some(serde_json::to_string(&message).unwrap());
                assert!(
                    target
                        .import_op_batch(&OpBatch {
                            from_organ: organ.clone(),
                            ops: vec![bad]
                        })
                        .await
                        .is_err()
                );
                assert_eq!(
                    store::records::quantity(&target.store.pool, &stock)
                        .await
                        .unwrap(),
                    quantity
                );
            }
            let batch = OpBatch {
                from_organ: organ.clone(),
                ops: vec![op],
            };
            assert_eq!(target.receive_sync_batch(&organ, &batch).await.unwrap(), 1);
            assert_eq!(target.receive_sync_batch(&organ, &batch).await.unwrap(), 0);
            assert!(target.batch_is_saved(&batch).await.unwrap());
        }
        assert_eq!(
            store::records::quantity(&target.store.pool, &stock)
                .await
                .unwrap()
                .unwrap()
                .to_f64(),
            8.0
        );
        assert_eq!(
            store::transfers::get(&target.store.pool, &fixture.transfer)
                .await
                .unwrap()
                .unwrap()
                .revision as u64,
            fixture.revision
        );
        let record = store::records::get(&target.store.pool, &fixture.transfer)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.kind, "transfer");
        assert!(!record.head.is_empty());
        let refused = target
            .require_transfer_origin_authority(&fixture.transfer)
            .await
            .unwrap_err();
        assert!(refused.to_string().contains("writing Cell"), "{refused}");
        let balance = store::transfer_balances::read(&target.store.pool, &stock)
            .await
            .unwrap();
        assert!(balance.reserved.is_zero());
        let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, &organ)
            .await
            .unwrap();
        assert!(
            source
                .export_sync_page(&organ, &vector, 2000)
                .await
                .unwrap()
                .batch
                .ops
                .is_empty()
        );
        let amount: f64 = store::sqlx::query_scalar(
            "SELECT delta FROM promise WHERE uid = 'precise-receive' AND transfer_uid = ?",
        )
        .bind(&precise.transfer)
        .fetch_one(&target.store.pool)
        .await
        .unwrap();
        assert_eq!(amount.to_bits(), precise_amount.to_bits());
        let contact = store::organs::contact(&target.store.pool, foreign_organ)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(contact.scope_fields, Some(vec!["head".into()]));
        assert_eq!(contact.accept_fields, Some(vec!["head".into()]));
        assert_eq!(contact.last_synced_seq, 0);
        assert_eq!(contact.peer_acked_seq, 0);
        assert!(
            store::visibility::hidden_from_organ(&target.store.pool, foreign_organ)
                .await
                .unwrap()
                .contains(&stock)
        );
        let foreign_page = target
            .export_sync_page(foreign_organ, &[], 2000)
            .await
            .unwrap();
        assert!(
            foreign_page
                .batch
                .ops
                .iter()
                .all(|op| op.tbl != "transfer_replication"
                    && op.uid != stock
                    && op.fact.as_ref().is_none_or(|fact| fact.record_uid != stock))
        );
        store::organs::set_trust(&source.store.pool, foreign_organ, "blocked")
            .await
            .unwrap();
        let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, &organ)
            .await
            .unwrap();
        let page = source
            .export_sync_page(&organ, &vector, 2000)
            .await
            .unwrap();
        target
            .receive_sync_batch(&organ, &page.batch)
            .await
            .unwrap();
        assert!(
            target
                .export_sync_page(foreign_organ, &[], 2000)
                .await
                .is_err()
        );
        let source_cell = store::cells::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let revoked = source.revoke_cell(&root, &source_cell).await.unwrap();
        target.adopt_roster(&revoked).await.unwrap();
        let before = store::records::quantity(&target.store.pool, &stock)
            .await
            .unwrap();
        let error = target
            .receive_sync_batch(
                &organ,
                &OpBatch {
                    from_organ: organ.clone(),
                    ops: vec![after_revocation],
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("writing authority"), "{error}");
        assert_eq!(
            store::records::quantity(&target.store.pool, &stock)
                .await
                .unwrap(),
            before
        );
    });
}
