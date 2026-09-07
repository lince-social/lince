use std::collections::BTreeSet;

use engine::access::{self, AccessError, AccessLimits, PreparedChanges};
use nucleus::{DecimalValue, RecordKind};
use protein::authority::{
    AssertionGrant, AssertionIntent, AssertionProperty, AssertionRole, AssertionState,
    AssertionTarget, AuthorityError, MutationGrant, MutationTarget, Operation, Property,
    RolePolicy,
};
use protein::{LinkDirection, Predicate};
use store::session_access::{self, DeviceAdmission};
use store::{Store, sqlx};

struct Fixture {
    store: Store,
    hosted: String,
    person: String,
    record: String,
    other: String,
    classification: String,
    relation: String,
    unit: String,
    assertion: String,
    role: i64,
    admission: DeviceAdmission,
}

fn all() -> Predicate {
    Predicate::All(Vec::new())
}
fn q(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}
fn target(uid: &str, properties: &[Property]) -> MutationTarget {
    MutationTarget {
        record_uid: uid.into(),
        touched_properties: properties.iter().cloned().collect(),
    }
}
fn intent(
    before: Option<AssertionState>,
    after: Option<AssertionState>,
    properties: &[AssertionProperty],
) -> AssertionIntent {
    AssertionIntent {
        before,
        after,
        touched_properties: properties.iter().copied().collect(),
    }
}

fn captured(row: &store::access_snapshot::AssertionMetadata) -> AssertionState {
    AssertionState {
        uid: row.uid.clone(),
        subject_uid: row.subject_uid.clone(),
        predicate_uid: row.predicate_uid.clone(),
        object_uid: row.object_uid.clone(),
        quantity: row.quantity,
        unit_uid: row.unit_uid.clone(),
        role: match row.role {
            store::access_snapshot::AssertionRole::Ordinary => AssertionRole::Ordinary,
            store::access_snapshot::AssertionRole::Identity => AssertionRole::Identity,
        },
    }
}

fn written(row: &store::assertions::AssertionRow) -> AssertionState {
    AssertionState {
        uid: row.uid.clone(),
        subject_uid: row.subject_uid.clone(),
        predicate_uid: row.predicate_uid.clone(),
        object_uid: row.object_uid.clone(),
        quantity: row.quantity,
        unit_uid: row.unit_uid.clone(),
        role: match row.role.as_str() {
            "ordinary" => AssertionRole::Ordinary,
            "identity" => AssertionRole::Identity,
            _ => panic!("unexpected writer role"),
        },
    }
}

async fn record(store: &Store, kind: RecordKind) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Head",
            body: "Body",
            quantity: q("0"),
        },
    )
    .await
    .unwrap()
    .uid
}

impl Fixture {
    async fn new() -> Self {
        let store = Store::open_memory().await.unwrap();
        let hosted = store::organs::local(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let person = record(&store, RecordKind::Person).await;
        let edited = record(&store, RecordKind::Plain).await;
        let other = record(&store, RecordKind::Plain).await;
        let classification = store::concepts::create(&store.pool, "Classification", &[])
            .await
            .unwrap();
        let relation = store::concepts::create(&store.pool, "Relationship", &[])
            .await
            .unwrap();
        let unit = store::concepts::create(&store.pool, "Unit", &[])
            .await
            .unwrap();
        let role = store::auth::ensure_role(&store.pool, "intent editor")
            .await
            .unwrap();
        store::auth::compare_and_set_role(&store.pool, &person, Some(role), 0)
            .await
            .unwrap();
        for action in ["read", "create", "update", "delete"] {
            let permission = store::auth::ensure_permission(&store.pool, "record", action)
                .await
                .unwrap();
            store::auth::grant(&store.pool, role, permission)
                .await
                .unwrap();
        }
        let peer = nucleus::new_uid("r");
        let node = format!("{:064x}", 5501);
        store::organs::add_contact(&store.pool, &peer, None, "Peer", "", 1)
            .await
            .unwrap();
        store::organs::set_node_id(&store.pool, &peer, Some(&node))
            .await
            .unwrap();
        store::logins::grant(&store.pool, &peer, &person)
            .await
            .unwrap();
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let authentication = session_access::granted_login_on(&mut tx, &peer, &node)
            .await
            .unwrap()
            .unwrap();
        let admission = session_access::register_device_on(&mut tx, &authentication, &node)
            .await
            .unwrap();
        let assertion = nucleus::new_uid("a");
        store::assertions::insert_tx(
            &mut tx,
            &assertion,
            store::assertions::NewAssertion {
                subject_uid: &edited,
                predicate_uid: &classification,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: Some(&person),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let fixture = Self {
            store,
            hosted,
            person,
            record: edited,
            other,
            classification,
            relation,
            unit,
            assertion,
            role,
            admission,
        };
        fixture.set_policy(fixture.policy()).await;
        fixture
    }

    fn policy(&self) -> RolePolicy {
        let rules = vec![
            AssertionGrant {
                predicate_uid: self.classification.clone(),
                target: AssertionTarget::Unary,
                role: AssertionRole::Ordinary,
                properties: BTreeSet::from([AssertionProperty::Quantity, AssertionProperty::Unit]),
            },
            AssertionGrant {
                predicate_uid: self.classification.clone(),
                target: AssertionTarget::Unary,
                role: AssertionRole::Identity,
                properties: BTreeSet::new(),
            },
            AssertionGrant {
                predicate_uid: self.relation.clone(),
                target: AssertionTarget::AnyReadableRecord,
                role: AssertionRole::Ordinary,
                properties: BTreeSet::from([AssertionProperty::Quantity, AssertionProperty::Unit]),
            },
        ];
        RolePolicy {
            read: all(),
            grants: [Operation::Update, Operation::Create]
                .into_iter()
                .map(|operation| MutationGrant {
                    operation,
                    selector: all(),
                    properties: BTreeSet::from([
                        Property::Kind,
                        Property::Head,
                        Property::Body,
                        Property::Slug,
                        Property::Quantity,
                        Property::Unit,
                        Property::Place,
                        Property::Organ,
                    ]),
                    assertions_add: rules.clone(),
                    assertions_remove: rules.clone(),
                })
                .collect(),
        }
    }

    async fn set_policy(&self, policy: RolePolicy) {
        let current = store::role_policies::get(&self.store.pool, self.role)
            .await
            .unwrap();
        store::role_policies::set(
            &self.store.pool,
            self.role,
            &serde_json::to_value(policy).unwrap(),
            current.map_or(0, |row| row.revision),
        )
        .await
        .unwrap();
    }

    async fn prepare<'a, 'b>(
        &self,
        tx: &'a mut sqlx::Transaction<'b, sqlx::Sqlite>,
        targets: &[String],
    ) -> PreparedChanges<'a, 'b> {
        access::prepare_changes_on(
            tx,
            &self.admission,
            &self.hosted,
            targets,
            AccessLimits::default(),
        )
        .await
        .unwrap()
    }

    async fn quantity(&self) -> Option<DecimalValue> {
        store::assertions::get(&self.store.pool, &self.assertion)
            .await
            .unwrap()
            .unwrap()
            .quantity
    }

    async fn revision(&self) -> i64 {
        let mut connection = self.store.pool.acquire().await.unwrap();
        store::record_revisions::get_on(&mut connection, &self.record)
            .await
            .unwrap()
            .revision
    }

    async fn sync_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_op")
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }

    async fn insert_relation(
        &self,
        access: &mut PreparedChanges<'_, '_>,
        uid: &str,
        object: &str,
    ) -> AssertionState {
        written(
            &store::assertions::insert_tx(
                access.transaction_for_staging(),
                uid,
                store::assertions::NewAssertion {
                    subject_uid: &self.record,
                    predicate_uid: &self.relation,
                    object_uid: Some(object),
                    role: store::assertions::AssertionRole::Ordinary,
                    quantity: None,
                    unit_uid: None,
                    asserted_by: Some(&self.person),
                },
            )
            .await
            .unwrap(),
        )
    }
}

#[tokio::test]
async fn access_assertion_intent_real_noop_clear_needs_explicit_both_side_properties() {
    let f = Fixture::new().await;
    let revision = f.revision().await;
    let sync = f.sync_count().await;
    for missing in [
        None,
        Some(AssertionProperty::Quantity),
        Some(AssertionProperty::Unit),
    ] {
        for add in [false, true] {
            let mut policy = f.policy();
            if let Some(property) = missing {
                let rules = if add {
                    &mut policy.grants[0].assertions_add
                } else {
                    &mut policy.grants[0].assertions_remove
                };
                for rule in rules {
                    rule.properties.remove(&property);
                }
            }
            f.set_policy(policy).await;
            let mut tx = store::write_tx(&f.store.pool).await.unwrap();
            let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
            let before = captured(access.current_assertion(&f.assertion).unwrap());
            let after = written(
                &store::assertions::set_quantity_tx(
                    access.transaction_for_staging(),
                    &f.assertion,
                    store::assertions::AssertionQuantity {
                        quantity: None,
                        unit_uid: None,
                    },
                )
                .await
                .unwrap(),
            );
            assert_eq!(before, after);
            let result = access
                .finish_changes_with_assertion_intents(
                    &[target(&f.record, &[])],
                    &[intent(
                        Some(before),
                        Some(after),
                        &[AssertionProperty::Quantity, AssertionProperty::Unit],
                    )],
                )
                .await;
            if missing.is_none() {
                let decision = result.unwrap();
                assert!(decision.records[&f.record].assertions_added.is_empty());
                assert_eq!(decision.revision(&f.record), Some(revision));
                tx.commit().await.unwrap();
            } else {
                assert!(matches!(
                    result,
                    Err(AccessError::Policy(AuthorityError::Denied))
                ));
                tx.rollback().await.unwrap();
            }
            assert_eq!(f.quantity().await, None);
            assert_eq!(f.revision().await, revision);
        }
    }
    assert_eq!(f.sync_count().await, sync);
}

#[tokio::test]
async fn access_assertion_intent_real_quantity_then_identity_promotion_uses_writer_results() {
    let f = Fixture::new().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    let before = captured(access.current_assertion(&f.assertion).unwrap());
    let quantified = written(
        &store::assertions::set_quantity_tx(
            access.transaction_for_staging(),
            &f.assertion,
            store::assertions::AssertionQuantity {
                quantity: Some(q("1.25")),
                unit_uid: Some(&f.unit),
            },
        )
        .await
        .unwrap(),
    );
    let promoted = written(
        &store::assertions::promote_identity_tx(access.transaction_for_staging(), &f.assertion)
            .await
            .unwrap(),
    );
    assert_eq!(promoted.quantity, None);
    assert_eq!(promoted.unit_uid, None);
    let intents = [
        intent(
            Some(before),
            Some(quantified.clone()),
            &[AssertionProperty::Quantity, AssertionProperty::Unit],
        ),
        intent(
            Some(quantified),
            Some(promoted.clone()),
            &[AssertionProperty::Quantity, AssertionProperty::Unit],
        ),
    ];
    let decision = access
        .finish_changes_with_assertion_intents(&[target(&f.record, &[])], &intents)
        .await
        .unwrap();
    assert!(
        decision.records[&f.record]
            .assertions_added
            .contains(&f.assertion)
    );
    tx.commit().await.unwrap();
    assert_eq!(
        written(
            &store::assertions::get(&f.store.pool, &f.assertion)
                .await
                .unwrap()
                .unwrap()
        ),
        promoted
    );
    let mut policy = f.policy();
    policy.grants[0]
        .assertions_remove
        .retain(|rule| rule.role != AssertionRole::Identity);
    f.set_policy(policy).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    let after = written(
        &store::assertions::promote_identity_tx(access.transaction_for_staging(), &f.assertion)
            .await
            .unwrap(),
    );
    assert!(matches!(
        access
            .finish_changes_with_assertion_intents(
                &[target(&f.record, &[])],
                &[intent(Some(promoted), Some(after), &[])]
            )
            .await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_assertion_intent_missing_or_forged_transition_rolls_back_real_effects() {
    let f = Fixture::new().await;
    let revision = f.revision().await;
    let sync = f.sync_count().await;
    for forgery in 0..4 {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
        let before = captured(access.current_assertion(&f.assertion).unwrap());
        let after = written(
            &store::assertions::set_quantity_tx(
                access.transaction_for_staging(),
                &f.assertion,
                store::assertions::AssertionQuantity {
                    quantity: Some(q("2")),
                    unit_uid: None,
                },
            )
            .await
            .unwrap(),
        );
        let mut intents = vec![intent(
            Some(before),
            Some(after),
            &[AssertionProperty::Quantity],
        )];
        match forgery {
            0 => intents.clear(),
            1 => intents[0].before.as_mut().unwrap().quantity = Some(q("9")),
            2 => intents[0].after.as_mut().unwrap().quantity = Some(q("3")),
            _ => intents[0].after.as_mut().unwrap().subject_uid = f.other.clone(),
        }
        assert!(matches!(
            access
                .finish_changes_with_assertion_intents(&[target(&f.record, &[])], &intents)
                .await,
            Err(AccessError::Policy(AuthorityError::InvalidMutation))
        ));
        tx.rollback().await.unwrap();
        assert_eq!(f.quantity().await, None);
        assert_eq!(f.revision().await, revision);
        assert_eq!(f.sync_count().await, sync);
    }
}

#[tokio::test]
async fn access_assertion_intent_canceled_real_insert_retract_retains_attempted_authority() {
    let f = Fixture::new().await;
    for permitted in [false, true] {
        let mut policy = f.policy();
        if !permitted {
            policy.grants[0]
                .assertions_remove
                .retain(|rule| rule.predicate_uid != f.relation);
        }
        f.set_policy(policy).await;
        let uid = nucleus::new_uid("a");
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
        let inserted = f.insert_relation(&mut access, &uid, &f.other).await;
        store::assertions::retract_tx(access.transaction_for_staging(), &uid, Some(&f.person))
            .await
            .unwrap();
        let result = access
            .finish_changes_with_assertion_intents(
                &[target(&f.record, &[])],
                &[
                    intent(None, Some(inserted.clone()), &[]),
                    intent(Some(inserted), None, &[]),
                ],
            )
            .await;
        if permitted {
            let decision = result.unwrap();
            assert!(decision.records[&f.record].assertions_added.is_empty());
            assert!(decision.records[&f.record].assertions_removed.is_empty());
            tx.commit().await.unwrap();
            assert!(
                store::assertions::get(&f.store.pool, &uid)
                    .await
                    .unwrap()
                    .unwrap()
                    .retracted_at
                    .is_some()
            );
        } else {
            match result {
                Err(error) => assert!(
                    matches!(error, AccessError::Policy(AuthorityError::Denied)),
                    "{error}"
                ),
                Ok(_) => panic!("unauthorized canceled Assertion change was accepted"),
            }
            tx.rollback().await.unwrap();
            assert!(
                store::assertions::get(&f.store.pool, &uid)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
async fn access_assertion_intent_scalar_and_noop_assertion_cannot_borrow_grants() {
    let f = Fixture::new().await;
    let mut split = f.policy();
    let mut assertion_only = split.grants[0].clone();
    assertion_only.properties.clear();
    split.grants[0].assertions_add.clear();
    split.grants[0].assertions_remove.clear();
    split.grants.push(assertion_only);
    f.set_policy(split).await;
    let revision = f.revision().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    let before = captured(access.current_assertion(&f.assertion).unwrap());
    store::records::set_authoring_text_on(
        access.transaction_for_staging(),
        &f.record,
        None,
        Some("Changed"),
    )
    .await
    .unwrap();
    assert!(matches!(
        access
            .finish_changes_with_assertion_intents(
                &[target(&f.record, &[Property::Body])],
                &[intent(
                    Some(before.clone()),
                    Some(before),
                    &[AssertionProperty::Unit]
                )]
            )
            .await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    assert_eq!(
        store::records::get(&f.store.pool, &f.record)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Body"
    );
    assert_eq!(f.revision().await, revision);
}

#[tokio::test]
async fn access_assertion_intent_new_cross_record_reference_is_validated_in_actual_proposed_state()
{
    let f = Fixture::new().await;
    let created = nucleus::new_uid("r");
    let uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f
        .prepare(&mut tx, &[f.record.clone(), created.clone()])
        .await;
    assert!(access.current_target(&created).is_none());
    store::records::create_with_uid_on(
        access.transaction_for_staging(),
        &created,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "New",
            body: "",
            quantity: q("0"),
        },
        &f.hosted,
        None,
    )
    .await
    .unwrap();
    let added = f.insert_relation(&mut access, &uid, &created).await;
    let decision = access
        .finish_changes_with_assertion_intents(
            &[target(&f.record, &[]), target(&created, &[])],
            &[intent(None, Some(added), &[])],
        )
        .await
        .unwrap();
    assert_eq!(decision.records[&created].operation, Operation::Create);
    assert!(decision.readable.records.contains(&created));
    assert!(decision.readable.assertions.contains(&uid));
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn access_assertion_intent_captured_targets_are_readonly_not_disclosure_grants() {
    let f = Fixture::new().await;
    sqlx::query("INSERT INTO visibility_rule (uid, subject_kind, target_uid, grant_level) VALUES (?, 'public', ?, 'hidden')")
        .bind(nucleus::new_uid("v")).bind(&f.record).execute(&f.store.pool).await.unwrap();
    for deleted in [false, true] {
        if deleted {
            store::records::mark_deleted(&f.store.pool, &f.record)
                .await
                .unwrap();
        }
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
        assert_eq!(access.current_target(&f.record).unwrap().deleted, deleted);
        assert_eq!(access.current_target(&f.record).unwrap().body, "Body");
        assert!(access.current_target(&f.other).is_none());
        assert!(access.readable_target(&f.record).is_none());
        assert_eq!(
            access.current_assertion(&f.assertion).unwrap().subject_uid,
            f.record
        );
        assert!(access.current_assertion(&nucleus::new_uid("a")).is_none());
        drop(access);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn access_assertion_intent_noop_target_never_exempts_indirect_authority_impact() {
    let f = Fixture::new().await;
    let dependency_role = store::auth::ensure_role(&f.store.pool, "dependent selector")
        .await
        .unwrap();
    store::role_policies::set(
        &f.store.pool,
        dependency_role,
        &serde_json::to_value(RolePolicy {
            read: Predicate::Relation {
                kind: f.relation.clone(),
                direction: LinkDirection::In,
                other: Some(f.record.clone()),
            },
            grants: Vec::new(),
        })
        .unwrap(),
        0,
    )
    .await
    .unwrap();
    let other_assertion = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::assertions::insert_tx(
        &mut tx,
        &other_assertion,
        store::assertions::NewAssertion {
            subject_uid: &f.other,
            predicate_uid: &f.classification,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: Some(&f.person),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for assigned in [false, true] {
        if assigned {
            let permission = store::auth::ensure_permission(&f.store.pool, "permission", "assign")
                .await
                .unwrap();
            store::auth::grant(&f.store.pool, f.role, permission)
                .await
                .unwrap();
        }
        let relation = nucleus::new_uid("a");
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = f
            .prepare(&mut tx, &[f.record.clone(), f.other.clone()])
            .await;
        let unchanged = captured(access.current_assertion(&other_assertion).unwrap());
        let added = f.insert_relation(&mut access, &relation, &f.other).await;
        let intents = [
            intent(None, Some(added), &[]),
            intent(
                Some(unchanged.clone()),
                Some(unchanged),
                &[AssertionProperty::Quantity],
            ),
        ];
        let result = access
            .finish_changes_with_assertion_intents(
                &[target(&f.record, &[]), target(&f.other, &[])],
                &intents,
            )
            .await;
        if assigned {
            let decision = result.unwrap();
            assert!(decision.records[&f.other].assertions_added.is_empty());
            tx.commit().await.unwrap();
        } else {
            assert!(matches!(
                result,
                Err(AccessError::Policy(AuthorityError::Denied))
            ));
            tx.rollback().await.unwrap();
            assert!(
                store::assertions::get(&f.store.pool, &relation)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
async fn access_assertion_intent_rechecks_current_authority_and_rolls_back_staging() {
    let f = Fixture::new().await;
    let current_policy = store::role_policies::get(&f.store.pool, f.role)
        .await
        .unwrap()
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    let before = captured(access.current_assertion(&f.assertion).unwrap());
    let after = written(
        &store::assertions::set_quantity_tx(
            access.transaction_for_staging(),
            &f.assertion,
            store::assertions::AssertionQuantity {
                quantity: Some(q("3")),
                unit_uid: None,
            },
        )
        .await
        .unwrap(),
    );
    store::role_policies::set_on(
        access.transaction_for_staging(),
        f.role,
        &serde_json::to_value(f.policy()).unwrap(),
        current_policy.revision,
    )
    .await
    .unwrap();
    assert!(matches!(
        access
            .finish_changes_with_assertion_intents(
                &[target(&f.record, &[])],
                &[intent(
                    Some(before),
                    Some(after),
                    &[AssertionProperty::Quantity]
                )]
            )
            .await,
        Err(AccessError::AuthorityChanged)
    ));
    tx.rollback().await.unwrap();
    assert_eq!(f.quantity().await, None);
    assert_eq!(
        store::role_policies::get(&f.store.pool, f.role)
            .await
            .unwrap()
            .unwrap()
            .revision,
        current_policy.revision
    );
}

#[tokio::test]
async fn access_assertion_intent_empty_checked_sequence_cannot_skip_actual_assertion_writes() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("a");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    f.insert_relation(&mut access, &uid, &f.other).await;
    assert!(matches!(
        access
            .finish_changes_with_assertion_intents(&[target(&f.record, &[])], &[])
            .await,
        Err(AccessError::Policy(AuthorityError::InvalidMutation))
    ));
    tx.rollback().await.unwrap();
    assert!(
        store::assertions::get(&f.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = f.prepare(&mut tx, std::slice::from_ref(&f.record)).await;
    store::records::set_authoring_text_on(
        access.transaction_for_staging(),
        &f.record,
        Some("New Head"),
        None,
    )
    .await
    .unwrap();
    assert!(
        access
            .finish_changes_with_assertion_intents(&[target(&f.record, &[Property::Head])], &[])
            .await
            .is_ok()
    );
    tx.commit().await.unwrap();
}
