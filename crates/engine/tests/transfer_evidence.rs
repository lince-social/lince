pub mod support;

use engine::{Engine, actions::Action};
use nucleus::transfer::outcome::{Content, Measure, NeedSelection, Request, Role};
use protein::{Protein, Source};
use serde_json::{Value, json};
use support::{DraftOptions, Person, TransferFixture};

struct Fixture {
    engine: Engine,
    giver: Person,
    receiver: Person,
    stranger: Person,
    transfer: TransferFixture,
    occurrence: String,
    stock: String,
    selection: NeedSelection,
    measure: Measure,
}

impl Fixture {
    async fn new() -> Self {
        let engine = support::engine().await;
        let giver = support::person(&engine, "outcomes.giver").await;
        let receiver = support::person(&engine, "outcomes.receiver").await;
        let stranger = support::person(&engine, "outcomes.stranger").await;
        let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
            .await
            .unwrap();
        for (resource, action) in [
            ("record", "create"),
            ("record", "read"),
            ("record", "update"),
            ("transfer", "read"),
            ("transfer", "update"),
            ("fact", "read"),
        ] {
            let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
        for person in [&giver, &receiver, &stranger] {
            store::auth::create_credential(
                &engine.store.pool,
                &person.uid,
                &person.uid,
                "hash",
                role,
            )
            .await
            .unwrap();
        }
        engine.set_signer(receiver.signer.clone()).await.unwrap();
        let need = engine
            .act(
                Action::CreateRecord {
                    slug: Some("outcomes.need".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Food for this week".into(),
                    body: String::new(),
                    quantity: -10.0,
                },
                Some(receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let vocabulary = store::linguas::create(&engine.store.pool, "Meals", None, "private")
            .await
            .unwrap();
        let concept = store::concepts::create_in(&engine.store.pool, &vocabulary, "meal", &[])
            .await
            .unwrap();
        let unit = store::concepts::create_in(&engine.store.pool, &vocabulary, "portion", &[])
            .await
            .unwrap();
        let assertion = store::assertions::assert(
            &engine.store.pool,
            store::assertions::NewAssertion {
                subject_uid: &need,
                predicate_uid: &concept,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: Some(store::exact::integer(10)),
                unit_uid: Some(&unit),
                asserted_by: Some(&receiver.uid),
            },
        )
        .await
        .unwrap();
        let revision = store::record_revisions::get_on(
            &mut *engine.store.pool.acquire().await.unwrap(),
            &need,
        )
        .await
        .unwrap()
        .revision;
        let selection = NeedSelection {
            need,
            expected_need_revision: revision,
            assertion,
            vocabulary: vocabulary.clone(),
            unit_vocabulary: Some(vocabulary),
        };
        let terms = store::transfer_evidence::freeze_need_on(
            &mut *engine.store.pool.acquire().await.unwrap(),
            &receiver.uid,
            &selection,
        )
        .await
        .unwrap();
        let stock = support::plain(&engine, "outcomes.stock", 20.0).await;
        let transfer = support::create_transfer(
            &engine,
            &giver,
            std::slice::from_ref(&receiver),
            vec![support::promise("outcome-food", &stock, &giver, -10.0)],
            DraftOptions {
                slug: "outcomes.transfer",
                agreement: nucleus::transfer::AgreementType::Full,
                reserve_default: engine::actions::TransferReservePoint::Active,
                require_confirmation: true,
            },
        )
        .await;
        support::agree(&engine, &transfer, &giver, "outcomes:agree:giver").await;
        support::agree(&engine, &transfer, &receiver, "outcomes:agree:receiver").await;
        let occurrence = support::activate(
            &engine,
            &transfer,
            &giver,
            "outcome-food",
            "outcomes:activate",
        )
        .await;
        Self {
            engine,
            giver,
            receiver,
            stranger,
            transfer,
            occurrence,
            stock,
            selection,
            measure: terms.measure,
        }
    }

    fn action(&self, person: &Person, id: &str, request: Request) -> Action {
        Action::RecordTransferOutcome {
            occurrence: self.occurrence.clone(),
            expected_revision: self.transfer.revision,
            request_id: id.into(),
            person: Some(person.uid.clone()),
            request,
        }
    }

    async fn bind(&self, id: &str) -> String {
        self.engine
            .set_signer(self.receiver.signer.clone())
            .await
            .unwrap();
        self.engine
            .act(
                self.action(
                    &self.receiver,
                    id,
                    Request::BindNeed {
                        selection: self.selection.clone(),
                        expected_measure: self.measure.clone(),
                        supersedes: None,
                    },
                ),
                Some(self.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap()
    }

    fn statement(
        &self,
        role: Role,
        amount: &str,
        binding: Option<&str>,
        supersedes: Option<&str>,
    ) -> Request {
        Request::State {
            role,
            measure: self.measure.clone(),
            amount: amount.into(),
            withdrawn: false,
            need_binding: binding.map(str::to_owned),
            supersedes: supersedes.map(str::to_owned),
        }
    }

    async fn history(&self, person: &Person) -> Value {
        let query = Protein {
            source: Source::Transfer,
            filter: vec![protein::Predicate::UidEq(self.transfer.transfer.clone())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let rows = protein::execute_for_with_signer(
            &self.engine.store,
            &query,
            Some(&person.uid),
            Some(&person.uid),
        )
        .await
        .unwrap();
        rows.into_iter()
            .find(|row| row["uid"] == self.transfer.transfer)
            .unwrap()["occurrences"][0]
            .clone()
    }
}

#[test]
fn receiver_need_and_actual_receipt_keep_distinct_exact_amounts_and_pinned_meanings() {
    support::run_async_test("outcome-exact-need", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("need:bind").await;
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        let giver_report = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.giver,
                    "giver:hundred",
                    fixture.statement(Role::Giver, "100", None, None),
                ),
                Some(fixture.giver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let action = fixture.action(
            &fixture.receiver,
            "receipt:twelve",
            fixture.statement(Role::Receiver, "12", Some(&binding), None),
        );
        let receipt = fixture
            .engine
            .act(action.clone(), Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        let uid = receipt.created.clone().unwrap();
        let saved = store::transfer_evidence::get(&fixture.engine.store.pool, &uid)
            .await
            .unwrap();
        assert!(
            engine::trust::verify_fact(&fixture.engine.store, &saved.fact)
                .await
                .unwrap()
        );
        assert!(
            matches!(saved.evidence.content, Content::Statement { quantity, .. } if quantity == store::exact::integer(12))
        );
        let need = store::transfer_evidence::get(&fixture.engine.store.pool, &binding)
            .await
            .unwrap();
        assert!(
            matches!(need.evidence.content, Content::NeedBinding { terms, .. } if terms.requested == store::exact::integer(10))
        );
        let progress = store::transfers::occurrence_settlement_progress(
            &fixture.engine.store.pool,
            &fixture.occurrence,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(progress.settled_quantity, 0.0);
        let occurrence =
            store::transfers::occurrence(&fixture.engine.store.pool, &fixture.occurrence)
                .await
                .unwrap()
                .unwrap();
        assert!(!occurrence.receipt_claimed);
        assert!(!occurrence.delivery_claimed);
        assert_eq!(
            store::records::get(&fixture.engine.store.pool, &fixture.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(20)
        );
        let replay = fixture
            .engine
            .act(action, Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        assert_eq!(replay.created, receipt.created);
        assert!(replay.facts.is_empty());
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        store::vocabulary::revise_definition_on(
            &mut tx,
            &fixture.measure.meaning.concept_uid,
            1,
            "Different meal definition",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let saved = store::transfer_evidence::get(&fixture.engine.store.pool, &uid)
            .await
            .unwrap();
        assert_eq!(saved.evidence.content.measure(), &fixture.measure);
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["outcomes"]["entries"].as_array().unwrap().len(), 2);
        assert_eq!(read["settlement_progress"]["settled_quantity"], 0.0);
        assert_eq!(read["outcome_measure_options"][0]["need_binding"], binding);
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                persons: vec![fixture.giver.uid.clone(), fixture.receiver.uid.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };
        store::sqlx::query(
            "UPDATE data_visibility SET policy_json=? WHERE record_uid=? AND data='record'",
        )
        .bind(serde_json::to_string(&policy).unwrap())
        .bind(giver_report)
        .execute(&fixture.engine.store.pool)
        .await
        .unwrap();
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["outcomes"]["entries"].as_array().unwrap().len(), 3);
        assert!(
            read["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["evidence"]["content"]["role"] == "giver"
                    && entry["evidence"]["content"]["quantity"]["value"] == "100")
        );
        support::claim(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.giver,
            engine::actions::TransferOccurrenceClaimRole::Delivery,
            "outcomes:completed:giver",
        )
        .await;
        support::claim(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.receiver,
            engine::actions::TransferOccurrenceClaimRole::Receipt,
            "outcomes:completed:receiver",
        )
        .await;
        let preview =
            support::settlement_preview(&fixture.engine, &fixture.occurrence, &fixture.giver, 10.0)
                .await;
        support::settle_from_preview(
            &fixture.engine,
            &fixture.occurrence,
            &fixture.giver,
            "outcomes:settled",
            &preview,
        )
        .await;
        let read = fixture.history(&fixture.receiver).await;
        assert_eq!(read["settlement_progress"]["settled_quantity"], 10.0);
        let statements = read["outcomes"]["entries"].as_array().unwrap();
        assert!(
            statements
                .iter()
                .any(|entry| entry["evidence"]["content"]["quantity"]["value"] == "12")
        );
        assert_eq!(
            store::records::get(&fixture.engine.store.pool, &fixture.stock)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::integer(10)
        );
    });
}

#[test]
fn outcome_authority_rejects_impersonation_changed_meanings_and_other_peoples_needs() {
    support::run_async_test("outcome-authority", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.giver.signer.clone())
            .await
            .unwrap();
        let impersonation = fixture.action(
            &fixture.receiver,
            "forged-person",
            fixture.statement(Role::Receiver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(impersonation, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let wrong_role = fixture.action(
            &fixture.giver,
            "forged-role",
            fixture.statement(Role::Receiver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(wrong_role, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let binding = fixture.action(
            &fixture.giver,
            "forged-need",
            Request::BindNeed {
                selection: fixture.selection.clone(),
                expected_measure: fixture.measure.clone(),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(binding, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        let mut changed = fixture.measure.clone();
        changed.meaning.definition_hash = "0".repeat(64);
        let action = fixture.action(
            &fixture.giver,
            "forged-hash",
            Request::State {
                role: Role::Giver,
                measure: changed,
                amount: "100".into(),
                withdrawn: false,
                need_binding: None,
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(action, Some(fixture.giver.uid.clone()))
                .await
                .is_err()
        );
        fixture
            .engine
            .set_signer(fixture.stranger.signer.clone())
            .await
            .unwrap();
        let action = fixture.action(
            &fixture.stranger,
            "outsider",
            fixture.statement(Role::Giver, "100", None, None),
        );
        assert!(
            fixture
                .engine
                .act(action, Some(fixture.stranger.uid.clone()))
                .await
                .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn corrections_keep_conflicts_and_withdrawals_without_duplicate_credit_or_replay_side_effects() {
    support::run_async_test("outcome-corrections", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let first = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "first",
                    fixture.statement(
                        Role::Receiver,
                        "9007199254740993.000000000000000001",
                        None,
                        None,
                    ),
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let duplicate = fixture.action(
            &fixture.receiver,
            "duplicate",
            fixture.statement(Role::Receiver, "8", None, None),
        );
        assert!(
            fixture
                .engine
                .act(duplicate, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        for (id, amount) in [("correction-a", "8"), ("correction-b", "9")] {
            fixture
                .engine
                .act(
                    fixture.action(
                        &fixture.receiver,
                        id,
                        fixture.statement(Role::Receiver, amount, None, Some(&first)),
                    ),
                    Some(fixture.receiver.uid.clone()),
                )
                .await
                .unwrap();
        }
        let history = fixture.history(&fixture.receiver).await;
        let entries = history["outcomes"]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["superseded"] == false && entry["conflicting"] == true)
                .count(),
            2
        );
        let old = entries
            .iter()
            .find(|entry| entry["evidence"]["record"] == first)
            .unwrap();
        assert_eq!(
            old["evidence"]["content"]["quantity"]["value"],
            "9007199254740993.000000000000000001"
        );
        let head = entries
            .iter()
            .find(|entry| entry["superseded"] == false)
            .unwrap()["evidence"]["record"]
            .as_str()
            .unwrap();
        let mut withdrawn = fixture.statement(Role::Receiver, "0", None, Some(head));
        if let Request::State { withdrawn, .. } = &mut withdrawn {
            *withdrawn = true;
        }
        fixture
            .engine
            .act(
                fixture.action(&fixture.receiver, "withdraw", withdrawn),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            fixture.history(&fixture.receiver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert!(
            store::sqlx::query(
                "UPDATE transfer_outcome_record SET payload_json='{}' WHERE record_uid=?"
            )
            .bind(&first)
            .execute(&fixture.engine.store.pool)
            .await
            .is_err()
        );
        assert!(
            store::sqlx::query("DELETE FROM transfer_outcome_record WHERE record_uid=?")
                .bind(&first)
                .execute(&fixture.engine.store.pool)
                .await
                .is_err()
        );
        let overwrite = Action::SetExtension {
            target: first,
            namespace: nucleus::transfer::outcome::NAMESPACE.into(),
            fds: json!({"quantity":"1000"}),
        };
        assert!(fixture.engine.act(overwrite, None).await.is_err());
        assert!(
            store::records::set_extension(
                &fixture.engine.store.pool,
                head,
                nucleus::transfer::outcome::NAMESPACE,
                &json!({"quantity":"1000"})
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn stale_need_reviews_and_mismatched_binding_units_are_rejected_atomically() {
    support::run_async_test("outcome-stale-meaning", || async {
        let fixture = Fixture::new().await;
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        store::sqlx::query("UPDATE record SET head='Updated Need' WHERE uid=?")
            .bind(&fixture.selection.need)
            .execute(&fixture.engine.store.pool)
            .await
            .unwrap();
        let stale = fixture.action(
            &fixture.receiver,
            "stale:need",
            Request::BindNeed {
                selection: fixture.selection.clone(),
                expected_measure: fixture.measure.clone(),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let mut selection = fixture.selection.clone();
        selection.expected_need_revision = store::record_revisions::get_on(
            &mut *fixture.engine.store.pool.acquire().await.unwrap(),
            &selection.need,
        )
        .await
        .unwrap()
        .revision;
        let binding = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "fresh:need",
                    Request::BindNeed {
                        selection,
                        expected_measure: fixture.measure.clone(),
                        supersedes: None,
                    },
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut measure = fixture.measure.clone();
        measure.unit = None;
        let wrong_unit = fixture.action(
            &fixture.receiver,
            "wrong:unit",
            Request::State {
                role: Role::Receiver,
                measure,
                amount: "10".into(),
                withdrawn: false,
                need_binding: Some(binding),
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(wrong_unit, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let stale = Action::RecordTransferOutcome {
            occurrence: fixture.occurrence.clone(),
            expected_revision: fixture.transfer.revision + 1,
            request_id: "stale:transfer".into(),
            person: Some(fixture.receiver.uid.clone()),
            request: fixture.statement(Role::Receiver, "10", None, None),
        };
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn a_reviewed_need_unit_cannot_change_silently_when_its_definition_changes() {
    support::run_async_test("outcome-reviewed-unit", || async {
        let fixture = Fixture::new().await;
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        store::assertions::retract_tx(
            &mut tx,
            &fixture.selection.assertion,
            Some(&fixture.receiver.uid),
        )
        .await
        .unwrap();
        let assertion = store::assertions::insert_tx(
            &mut tx,
            &nucleus::new_uid("a"),
            store::assertions::NewAssertion {
                subject_uid: &fixture.selection.need,
                predicate_uid: &fixture.measure.meaning.concept_uid,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: Some(&fixture.receiver.uid),
            },
        )
        .await
        .unwrap()
        .uid;
        store::sqlx::query("UPDATE record SET unit_uid=? WHERE uid=?")
            .bind(&fixture.measure.unit.as_ref().unwrap().concept_uid)
            .bind(&fixture.selection.need)
            .execute(&mut *tx)
            .await
            .unwrap();
        let selection = NeedSelection {
            assertion,
            expected_need_revision: store::record_revisions::get_on(
                &mut tx,
                &fixture.selection.need,
            )
            .await
            .unwrap()
            .revision,
            ..fixture.selection.clone()
        };
        let reviewed =
            store::transfer_evidence::freeze_need_on(&mut tx, &fixture.receiver.uid, &selection)
                .await
                .unwrap();
        store::vocabulary::revise_definition_on(
            &mut tx,
            &fixture.measure.unit.as_ref().unwrap().concept_uid,
            1,
            "A larger portion",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        fixture
            .engine
            .set_signer(fixture.receiver.signer.clone())
            .await
            .unwrap();
        let stale = fixture.action(
            &fixture.receiver,
            "stale:unit-review",
            Request::BindNeed {
                selection: selection.clone(),
                expected_measure: reviewed.measure,
                supersedes: None,
            },
        );
        assert!(
            fixture
                .engine
                .act(stale, Some(fixture.receiver.uid.clone()))
                .await
                .is_err()
        );
        let fresh = store::transfer_evidence::freeze_need_on(
            &mut *fixture.engine.store.pool.acquire().await.unwrap(),
            &fixture.receiver.uid,
            &selection,
        )
        .await
        .unwrap();
        let approved = fixture.action(
            &fixture.receiver,
            "fresh:unit-review",
            Request::BindNeed {
                selection,
                expected_measure: fresh.measure,
                supersedes: None,
            },
        );
        fixture
            .engine
            .act(approved, Some(fixture.receiver.uid.clone()))
            .await
            .unwrap();
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_outcome_record")
            .fetch_one(&fixture.engine.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    });
}

#[test]
fn outcome_records_start_private_and_unshared_need_mappings_do_not_escape_through_history() {
    support::run_async_test("outcome-private", || async {
        let fixture = Fixture::new().await;
        let binding = fixture.bind("private:bind").await;
        let receipt = fixture
            .engine
            .act(
                fixture.action(
                    &fixture.receiver,
                    "private:received",
                    fixture.statement(Role::Receiver, "10", Some(&binding), None),
                ),
                Some(fixture.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert_eq!(
            fixture.history(&fixture.receiver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            fixture.history(&fixture.giver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for record in [&binding, &receipt] {
            let raw: String = store::sqlx::query_scalar("SELECT value FROM sync_op WHERE tbl='data_visibility' AND uid=? AND field='record' AND kind='set' ORDER BY seq DESC LIMIT 1")
                .bind(record).fetch_one(&fixture.engine.store.pool).await.unwrap();
            let policy: nucleus::visibility::SavedPolicy = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                policy.policy.include[0].persons,
                vec![fixture.receiver.uid.clone()]
            );
            assert!(policy.policy.exclude.is_empty());
        }
        let mut tx = store::write_tx(&fixture.engine.store.pool).await.unwrap();
        let policy = nucleus::visibility::Policy {
            include: vec![nucleus::visibility::Condition {
                persons: vec![fixture.giver.uid.clone(), fixture.receiver.uid.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };
        for record in [&binding, &receipt] {
            store::sqlx::query(
                "UPDATE data_visibility SET policy_json=? WHERE record_uid=? AND data='record'",
            )
            .bind(serde_json::to_string(&policy).unwrap())
            .bind(record)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        store::data_visibility::ensure_private_for_person_on(
            &mut tx,
            &fixture.selection.need,
            nucleus::visibility::Data::Record,
            Some(&fixture.receiver.uid),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(
            fixture.history(&fixture.giver).await["outcomes"]["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let query = Protein {
            source: Source::Record,
            filter: vec![protein::Predicate::UidEq(receipt)],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        assert!(
            protein::execute_for(&fixture.engine.store, &query, Some(&fixture.stranger.uid))
                .await
                .unwrap()
                .is_empty()
        );
    });
}
