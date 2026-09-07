use std::collections::BTreeSet;

use engine::access::{self, AccessError, AccessLimits, PreparedChanges};
use nucleus::RecordKind;
use protein::authority::{
    AssertionGrant, AssertionRole, AssertionTarget, AuthorityError, MutationGrant, MutationTarget,
    Operation, Property, RolePolicy,
};
use protein::{LinkDirection, Predicate};
use serde_json::json;
use store::session_access::{self, DeviceAdmission};
use store::{Store, sqlx};

struct Fixture {
    store: Store,
    hosted: String,
    person: String,
    role: i64,
    admission: DeviceAdmission,
    edited: String,
    affected: String,
    relation: String,
}

fn all() -> Predicate {
    Predicate::All(Vec::new())
}

fn target(uid: &str, properties: &[Property]) -> MutationTarget {
    MutationTarget {
        record_uid: uid.into(),
        touched_properties: properties.iter().cloned().collect(),
    }
}

fn grant(operation: Operation, selector: Predicate) -> MutationGrant {
    MutationGrant {
        operation,
        selector,
        properties: [
            Property::Kind,
            Property::Slug,
            Property::Head,
            Property::Body,
            Property::Quantity,
            Property::Unit,
            Property::Place,
            Property::Organ,
        ]
        .into_iter()
        .collect(),
        assertions_add: Vec::new(),
        assertions_remove: Vec::new(),
    }
}

fn actor_policy(relation: &str) -> RolePolicy {
    let mut grants: Vec<_> = [
        Operation::Create,
        Operation::Update,
        Operation::Delete,
        Operation::Restore,
    ]
    .into_iter()
    .map(|operation| grant(operation, all()))
    .collect();
    let relationship = AssertionGrant {
        predicate_uid: relation.into(),
        target: AssertionTarget::AnyReadableRecord,
        role: AssertionRole::Ordinary,
        properties: BTreeSet::new(),
    };
    let update = grants
        .iter_mut()
        .find(|grant| grant.operation == Operation::Update)
        .unwrap();
    update.assertions_add.push(relationship.clone());
    update.assertions_remove.push(relationship);
    RolePolicy {
        read: all(),
        grants,
    }
}

async fn record(store: &Store, kind: RecordKind, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
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
        let person = record(&store, RecordKind::Person, "Operator").await;
        let edited = record(&store, RecordKind::Plain, "Edited").await;
        let affected = record(&store, RecordKind::Plain, "Affected").await;
        let relation = store::concepts::create(&store.pool, "Dependency relation", &[])
            .await
            .unwrap();
        let role = store::auth::ensure_role(&store.pool, "ordinary-dependency-editor")
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
        store::role_policies::set(
            &store.pool,
            role,
            &serde_json::to_value(actor_policy(&relation)).unwrap(),
            0,
        )
        .await
        .unwrap();
        let peer = nucleus::new_uid("r");
        store::organs::add_contact(&store.pool, &peer, None, "Peer", "", 1)
            .await
            .unwrap();
        let node = format!("{:064x}", 701);
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
        tx.commit().await.unwrap();
        Self {
            store,
            hosted,
            person,
            role,
            admission,
            edited,
            affected,
            relation,
        }
    }

    fn selector(&self) -> Predicate {
        Predicate::Relation {
            kind: self.relation.clone(),
            direction: LinkDirection::In,
            other: Some(self.edited.clone()),
        }
    }

    async fn dependency_policy(&self, policy: RolePolicy) -> i64 {
        let role = store::auth::ensure_role(
            &self.store.pool,
            &format!("dependency-{}", nucleus::new_uid("r")),
        )
        .await
        .unwrap();
        store::role_policies::set(
            &self.store.pool,
            role,
            &serde_json::to_value(policy).unwrap(),
            0,
        )
        .await
        .unwrap();
        role
    }

    async fn grant_assign(&self) {
        let permission = store::auth::ensure_permission(&self.store.pool, "permission", "assign")
            .await
            .unwrap();
        store::auth::grant(&self.store.pool, self.role, permission)
            .await
            .unwrap();
    }

    async fn prepare<'operation, 'database>(
        &self,
        tx: &'operation mut sqlx::Transaction<'database, sqlx::Sqlite>,
        targets: &[String],
        limits: AccessLimits,
    ) -> Result<PreparedChanges<'operation, 'database>, AccessError> {
        access::prepare_changes_on(tx, &self.admission, &self.hosted, targets, limits).await
    }
}

async fn record_revision(
    store: &Store,
    uid: &str,
) -> Result<store::record_revisions::RecordRevision, store::StoreError> {
    let mut connection = store.pool.acquire().await?;
    store::record_revisions::get_on(&mut connection, uid).await
}

async fn stage_relation(access: &mut PreparedChanges<'_, '_>, fixture: &Fixture) -> String {
    let uid = nucleus::new_uid("a");
    sqlx::query(
        "INSERT INTO record_assertion
             (uid, subject_uid, predicate_uid, object_uid, role, asserted_by, created_at)
         VALUES (?, ?, ?, ?, 'ordinary', ?, 'now')",
    )
    .bind(&uid)
    .bind(&fixture.edited)
    .bind(&fixture.relation)
    .bind(&fixture.affected)
    .bind(&fixture.person)
    .execute(&mut **access.transaction_for_staging())
    .await
    .unwrap();
    uid
}

async fn assert_relation_refused(fixture: &Fixture) {
    let before = record_revision(&fixture.store, &fixture.edited)
        .await
        .unwrap()
        .revision;
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.edited),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    let assertion = stage_relation(&mut access, fixture).await;
    assert!(matches!(
        access.finish_changes(&[target(&fixture.edited, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM record_assertion WHERE uid = ?")
            .bind(assertion)
            .fetch_optional(&fixture.store.pool)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        record_revision(&fixture.store, &fixture.edited)
            .await
            .unwrap()
            .revision,
        before
    );
}

#[tokio::test]
async fn every_role_read_and_operation_selector_protects_indirect_membership() {
    let cases = [
        None,
        Some(Operation::Create),
        Some(Operation::Update),
        Some(Operation::Delete),
        Some(Operation::Restore),
    ];
    for operation in cases {
        let fixture = Fixture::new().await;
        let selector = fixture.selector();
        let policy = match operation {
            None => RolePolicy {
                read: selector,
                grants: Vec::new(),
            },
            Some(operation) => RolePolicy {
                read: all(),
                grants: vec![grant(operation, selector)],
            },
        };
        fixture.dependency_policy(policy).await;
        assert_relation_refused(&fixture).await;
    }
}

#[tokio::test]
async fn credential_free_disabled_and_deleted_person_filters_are_dependencies() {
    for state in 0..3 {
        let fixture = Fixture::new().await;
        let person = record(&fixture.store, RecordKind::Person, "Filtered Person").await;
        store::auth::compare_and_set_role(&fixture.store.pool, &person, None, 0)
            .await
            .unwrap();
        let filter = serde_json::to_string(&fixture.selector()).unwrap();
        store::read_filter::set(&fixture.store.pool, &person, Some(&filter))
            .await
            .unwrap();
        assert!(
            !store::auth::has_credential(&fixture.store.pool, &person)
                .await
                .unwrap()
        );
        if state == 1 {
            store::people::deactivate(&fixture.store.pool, &person, "2026-09-07T00:00:00Z", None)
                .await
                .unwrap();
        } else if state == 2 {
            store::records::mark_deleted(&fixture.store.pool, &person)
                .await
                .unwrap();
        }
        assert_relation_refused(&fixture).await;
    }
}

#[tokio::test]
async fn permission_assign_allows_a_reviewed_indirect_dependency_change() {
    let fixture = Fixture::new().await;
    let selector = fixture.selector();
    fixture
        .dependency_policy(RolePolicy {
            read: selector.clone(),
            grants: [
                Operation::Create,
                Operation::Update,
                Operation::Delete,
                Operation::Restore,
            ]
            .into_iter()
            .map(|operation| grant(operation, selector.clone()))
            .collect(),
        })
        .await;
    let filtered = record(&fixture.store, RecordKind::Person, "Filtered").await;
    store::auth::compare_and_set_role(&fixture.store.pool, &filtered, None, 0)
        .await
        .unwrap();
    store::read_filter::set(
        &fixture.store.pool,
        &filtered,
        Some(&serde_json::to_string(&selector).unwrap()),
    )
    .await
    .unwrap();
    fixture.grant_assign().await;
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.edited),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    stage_relation(&mut access, &fixture).await;
    access
        .finish_changes(&[target(&fixture.edited, &[])])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn cancelling_selector_changes_do_not_hide_indirect_effects() {
    let fixture = Fixture::new().await;
    let selector = fixture.selector();
    fixture
        .dependency_policy(RolePolicy {
            read: selector.clone(),
            grants: vec![grant(Operation::Update, Predicate::Not(Box::new(selector)))],
        })
        .await;
    assert_relation_refused(&fixture).await;
}

#[tokio::test]
async fn unrelated_ordinary_edit_does_not_require_permission_assign() {
    let fixture = Fixture::new().await;
    fixture
        .dependency_policy(RolePolicy {
            read: fixture.selector(),
            grants: Vec::new(),
        })
        .await;
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.edited),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE record SET body = 'unrelated' WHERE uid = ?")
        .bind(&fixture.edited)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    access
        .finish_changes(&[target(&fixture.edited, &[])])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn declared_noop_and_fact_revision_do_not_exempt_an_indirect_record() {
    for with_fact in [false, true] {
        let fixture = Fixture::new().await;
        fixture
            .dependency_policy(RolePolicy {
                read: fixture.selector(),
                grants: Vec::new(),
            })
            .await;
        let before = record_revision(&fixture.store, &fixture.affected)
            .await
            .unwrap()
            .revision;
        let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
        let mut access = fixture
            .prepare(
                &mut tx,
                &[fixture.edited.clone(), fixture.affected.clone()],
                AccessLimits::default(),
            )
            .await
            .unwrap();
        let assertion = stage_relation(&mut access, &fixture).await;
        let fact = nucleus::new_uid("f");
        if with_fact {
            sqlx::query(
                "INSERT INTO fact
                     (uid, record_uid, delta_mantissa, delta_scale, at, actor_uid,
                      cause_kind, cause_uid, payload, prev_hash, hash, signature)
                 VALUES (?, ?, '0', 0, 'now', ?, 'user_edit', NULL, NULL,
                         'genesis', ?, NULL)",
            )
            .bind(&fact)
            .bind(&fixture.affected)
            .bind(&fixture.person)
            .bind(format!("hash-{fact}"))
            .execute(&mut **access.transaction_for_staging())
            .await
            .unwrap();
        }
        assert!(matches!(
            access
                .finish_changes(&[
                    target(&fixture.edited, &[]),
                    target(&fixture.affected, &[Property::Body]),
                ])
                .await,
            Err(AccessError::Policy(AuthorityError::Denied))
        ));
        tx.rollback().await.unwrap();
        assert!(
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM record_assertion WHERE uid = ?")
                .bind(assertion)
                .fetch_optional(&fixture.store.pool)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store::facts::get(&fixture.store.pool, &fact)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            record_revision(&fixture.store, &fixture.affected)
                .await
                .unwrap()
                .revision,
            before
        );
    }
}

#[tokio::test]
async fn actual_relationship_and_record_update_exempt_the_changed_records() {
    let fixture = Fixture::new().await;
    fixture
        .dependency_policy(RolePolicy {
            read: fixture.selector(),
            grants: Vec::new(),
        })
        .await;
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            &[fixture.edited.clone(), fixture.affected.clone()],
            AccessLimits::default(),
        )
        .await
        .unwrap();
    stage_relation(&mut access, &fixture).await;
    sqlx::query("UPDATE record SET body = 'semantic update' WHERE uid = ?")
        .bind(&fixture.affected)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    access
        .finish_changes(&[target(&fixture.edited, &[]), target(&fixture.affected, &[])])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn create_delete_and_restore_membership_changes_exempt_the_written_record() {
    let fixture = Fixture::new().await;
    let created = nucleus::new_uid("r");
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&created),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO record
             (uid, kind, head, body, quantity_mantissa, quantity_scale, organ_uid,
              created_at, updated_at)
         VALUES (?, 'plain', 'created', '', '0', 0, ?, 'now', 'now')",
    )
    .bind(&created)
    .bind(&fixture.hosted)
    .execute(&mut **access.transaction_for_staging())
    .await
    .unwrap();
    let decision = access
        .finish_changes(&[target(&created, &[])])
        .await
        .unwrap();
    assert_eq!(decision.records[&created].operation, Operation::Create);
    tx.rollback().await.unwrap();

    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.affected),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE record SET deleted_at = 'now' WHERE uid = ?")
        .bind(&fixture.affected)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    let decision = access
        .finish_changes(&[target(&fixture.affected, &[])])
        .await
        .unwrap();
    assert_eq!(
        decision.records[&fixture.affected].operation,
        Operation::Delete
    );
    tx.rollback().await.unwrap();

    store::records::mark_deleted(&fixture.store.pool, &fixture.affected)
        .await
        .unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.affected),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&fixture.affected)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    let decision = access
        .finish_changes(&[target(&fixture.affected, &[])])
        .await
        .unwrap();
    assert_eq!(
        decision.records[&fixture.affected].operation,
        Operation::Restore
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn null_catalog_rows_and_catalog_revisions_are_protected() {
    for policy_catalog in [true, false] {
        let fixture = Fixture::new().await;
        let null_role = store::auth::ensure_role(&fixture.store.pool, "null-policy")
            .await
            .unwrap();
        store::role_policies::compare_and_set(&fixture.store.pool, null_role, None, 0)
            .await
            .unwrap();
        let null_person = record(&fixture.store, RecordKind::Person, "Null access").await;
        store::auth::compare_and_set_role(&fixture.store.pool, &null_person, None, 0)
            .await
            .unwrap();
        let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
        let mut access = fixture
            .prepare(
                &mut tx,
                std::slice::from_ref(&fixture.edited),
                AccessLimits::default(),
            )
            .await
            .unwrap();
        sqlx::query("UPDATE record SET body = 'staged' WHERE uid = ?")
            .bind(&fixture.edited)
            .execute(&mut **access.transaction_for_staging())
            .await
            .unwrap();
        if policy_catalog {
            sqlx::query("UPDATE role_policy SET revision = revision + 1 WHERE role_id = ?")
                .bind(null_role)
                .execute(&mut **access.transaction_for_staging())
                .await
                .unwrap();
        } else {
            sqlx::query("UPDATE person_access SET revision = revision + 1 WHERE person_uid = ?")
                .bind(&null_person)
                .execute(&mut **access.transaction_for_staging())
                .await
                .unwrap();
        }
        assert!(matches!(
            access.finish_changes(&[target(&fixture.edited, &[])]).await,
            Err(AccessError::AuthorityChanged)
        ));
        tx.rollback().await.unwrap();
        assert_eq!(
            store::records::get(&fixture.store.pool, &fixture.edited)
                .await
                .unwrap()
                .unwrap()
                .body,
            ""
        );
    }
}

#[tokio::test]
async fn lightweight_reads_skip_unrelated_invalid_catalogs_but_preparation_refuses() {
    let fixture = Fixture::new().await;
    let role = store::auth::ensure_role(&fixture.store.pool, "invalid-policy")
        .await
        .unwrap();
    store::role_policies::set(
        &fixture.store.pool,
        role,
        &json!({"read": {"All": []}, "grants": [], "unknown": true}),
        0,
    )
    .await
    .unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    assert!(
        access::load_on(
            &mut tx,
            &fixture.admission,
            &fixture.hosted,
            &[],
            AccessLimits::default(),
        )
        .await
        .is_ok()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    assert!(matches!(
        fixture
            .prepare(
                &mut tx,
                std::slice::from_ref(&fixture.edited),
                AccessLimits::default(),
            )
            .await,
        Err(AccessError::InvalidAuthority)
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn unsupported_dependency_selector_refuses_even_when_unmatched() {
    let fixture = Fixture::new().await;
    fixture
        .dependency_policy(RolePolicy {
            read: Predicate::SlugEq("unmatched".into()),
            grants: Vec::new(),
        })
        .await;
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(
            &mut tx,
            std::slice::from_ref(&fixture.edited),
            AccessLimits::default(),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE record SET body = 'ordinary' WHERE uid = ?")
        .bind(&fixture.edited)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&fixture.edited, &[])]).await,
        Err(AccessError::Policy(AuthorityError::UnsupportedPredicate))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn all_dependency_selectors_share_the_authority_budget() {
    let fixture = Fixture::new().await;
    fixture
        .dependency_policy(RolePolicy {
            read: fixture.selector(),
            grants: Vec::new(),
        })
        .await;
    let limits = AccessLimits {
        authority: protein::authority::Limits {
            predicate_nodes: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let mut access = fixture
        .prepare(&mut tx, std::slice::from_ref(&fixture.edited), limits)
        .await
        .unwrap();
    sqlx::query("UPDATE record SET body = 'ordinary' WHERE uid = ?")
        .bind(&fixture.edited)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&fixture.edited, &[])]).await,
        Err(AccessError::Policy(AuthorityError::LimitExceeded))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn oversized_and_row_overflow_catalogs_refuse_before_staging() {
    let fixture = Fixture::new().await;
    let person = record(&fixture.store, RecordKind::Person, "Oversized filter").await;
    store::auth::compare_and_set_role(&fixture.store.pool, &person, None, 0)
        .await
        .unwrap();
    sqlx::query("UPDATE person_access SET read_filter = ? WHERE person_uid = ?")
        .bind("x".repeat(store::auth::MAX_READ_FILTER_BYTES + 1))
        .bind(&person)
        .execute(&fixture.store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    assert!(matches!(
        fixture
            .prepare(
                &mut tx,
                std::slice::from_ref(&fixture.edited),
                AccessLimits::default(),
            )
            .await,
        Err(AccessError::Storage)
    ));
    tx.rollback().await.unwrap();

    let fixture = Fixture::new().await;
    sqlx::query(
        "WITH digits(value) AS (
             VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
         )
         INSERT INTO role (name)
         SELECT 'dependency-bound-' ||
                (a.value + 10 * b.value + 100 * c.value + 1000 * d.value)
           FROM digits a, digits b, digits c, digits d
          WHERE a.value + 10 * b.value + 100 * c.value + 1000 * d.value <= ?",
    )
    .bind(i64::try_from(store::role_policies::MAX_POLICY_ROWS).unwrap())
    .execute(&fixture.store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO role_policy (role_id, policy, revision)
         SELECT id, NULL, 1 FROM role WHERE name LIKE 'dependency-bound-%'",
    )
    .execute(&fixture.store.pool)
    .await
    .unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    assert!(matches!(
        fixture
            .prepare(
                &mut tx,
                std::slice::from_ref(&fixture.edited),
                AccessLimits::default(),
            )
            .await,
        Err(AccessError::Storage)
    ));
    tx.rollback().await.unwrap();
}
