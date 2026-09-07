use std::collections::BTreeSet;

use engine::access::{self, AccessError, AccessLimits, PreparedChanges, ReadableSets};
use nucleus::RecordKind;
use protein::Predicate;
use protein::authority::{
    AssertionGrant, AssertionRole, AssertionTarget, AuthorityError, ExtensionProperty,
    MutationGrant, MutationTarget, Operation, Property, RolePolicy,
};
use serde_json::json;
use store::session_access::{self, DeviceAdmission};
use store::{Store, sqlx};

struct Fixture {
    store: Store,
    hosted: String,
    person: String,
    other: String,
    peer: String,
    role: i64,
    admission: DeviceAdmission,
    record: String,
}

fn all() -> Predicate {
    Predicate::All(Vec::new())
}

fn policy() -> RolePolicy {
    RolePolicy {
        read: all(),
        grants: [
            Operation::Create,
            Operation::Update,
            Operation::Delete,
            Operation::Restore,
        ]
        .into_iter()
        .map(|operation| MutationGrant {
            operation,
            selector: all(),
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
        })
        .collect(),
    }
}

async fn record(store: &Store, kind: RecordKind) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Visible title",
            body: "Initial body",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

impl Fixture {
    async fn new() -> Self {
        Self::with_store(Store::open_memory().await.unwrap()).await
    }

    async fn with_store(store: Store) -> Self {
        let hosted = store::organs::local(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let person = record(&store, RecordKind::Person).await;
        let other = record(&store, RecordKind::Person).await;
        let record = record(&store, RecordKind::Plain).await;
        let role = store::auth::ensure_role(&store.pool, "ordinary-role-not-a-selector")
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
            &serde_json::to_value(policy()).unwrap(),
            0,
        )
        .await
        .unwrap();
        let peer = nucleus::new_uid("r");
        store::organs::add_contact(&store.pool, &peer, None, "Connecting Organ", "", 1)
            .await
            .unwrap();
        let node = format!("{:064x}", 123);
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
            other,
            peer,
            role,
            admission,
            record,
        }
    }

    async fn set_policy(&self, policy: RolePolicy) {
        let previous = store::role_policies::get(&self.store.pool, self.role)
            .await
            .unwrap()
            .unwrap();
        store::role_policies::set(
            &self.store.pool,
            self.role,
            &serde_json::to_value(policy).unwrap(),
            previous.revision,
        )
        .await
        .unwrap();
    }

    async fn read(&self) -> Result<ReadableSets, AccessError> {
        let mut tx = store::write_tx(&self.store.pool).await.unwrap();
        let result = access::load_on(
            &mut tx,
            &self.admission,
            &self.hosted,
            &[],
            AccessLimits::default(),
        )
        .await
        .map(|access| access.readable().clone());
        tx.rollback().await.unwrap();
        result
    }

    async fn revoke_permission(&self, action: &str) {
        let id = store::auth::ensure_permission(&self.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::revoke(&self.store.pool, self.role, id)
            .await
            .unwrap();
    }
}

async fn visibility(
    f: &Fixture,
    target: &str,
    subject_kind: &str,
    subject: Option<&str>,
    grant: &str,
    field: Option<&str>,
) {
    sqlx::query("INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level, field)
                 VALUES (?, ?, ?, ?, ?, ?)")
        .bind(nucleus::new_uid("v")).bind(subject_kind).bind(subject).bind(target).bind(grant).bind(field)
        .execute(&f.store.pool).await.unwrap();
}

fn target(uid: &str, footprint: &[Property]) -> MutationTarget {
    MutationTarget {
        record_uid: uid.into(),
        touched_properties: footprint.iter().cloned().collect(),
    }
}

async fn body(access: &mut PreparedChanges<'_, '_>, uid: &str, body: &str) {
    sqlx::query("UPDATE record SET body = ? WHERE uid = ?")
        .bind(body)
        .bind(uid)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
}

async fn assertion(
    f: &Fixture,
    subject: &str,
    predicate: &str,
    object: Option<&str>,
    unit: Option<&str>,
) -> String {
    store::assertions::assert(
        &f.store.pool,
        store::assertions::NewAssertion {
            subject_uid: subject,
            predicate_uid: predicate,
            object_uid: object,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: unit,
            asserted_by: Some(&f.person),
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn access_boundary_credential_free_person_reads_local_records_without_fact_authorship() {
    let f = Fixture::new().await;
    assert!(
        !store::auth::has_credential(&f.store.pool, &f.person)
            .await
            .unwrap()
    );
    assert!(
        !store::visibility::visible_targets(&f.store.pool, &f.person)
            .await
            .unwrap()
            .contains(&f.record)
    );
    let sets = f.read().await.unwrap();
    assert!(sets.records.contains(&f.record));
    assert!(sets.records.contains(&f.hosted));
    assert!(!sets.records.contains(&f.peer));
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let access = access::load_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    assert_eq!(access.person_uid(), f.person);
    assert_eq!(access.hosted_organ(), f.hosted);
    assert_eq!(access.peer_organ(), Some(f.peer.as_str()));
    assert!(access.permits("record:read"));
    assert!(access.require_permission("permission:assign").is_err());
    assert_eq!(
        access.readable_target(&f.record).unwrap().body,
        "Initial body"
    );
    assert!(access.readable_target(&f.peer).is_none());
    drop(access);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_missing_role_or_policy_and_invalid_policy_grant_nothing() {
    let f = Fixture::new().await;
    let assignment = store::auth::person_access(&f.store.pool, &f.person)
        .await
        .unwrap()
        .unwrap();
    store::auth::compare_and_set_role(&f.store.pool, &f.person, None, assignment.revision)
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::MissingAuthority)));
    let assignment = store::auth::person_access(&f.store.pool, &f.person)
        .await
        .unwrap()
        .unwrap();
    let empty = store::auth::ensure_role(&f.store.pool, "admin")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&f.store.pool, &f.person, Some(empty), assignment.revision)
        .await
        .unwrap();
    assert!(f.read().await.is_err());
    let assignment = store::auth::person_access(&f.store.pool, &f.person)
        .await
        .unwrap()
        .unwrap();
    store::auth::compare_and_set_role(&f.store.pool, &f.person, Some(f.role), assignment.revision)
        .await
        .unwrap();
    for value in [
        json!({}),
        json!({"read": {"All": []}, "grants": [], "unknown": true}),
    ] {
        let previous = store::role_policies::get(&f.store.pool, f.role)
            .await
            .unwrap()
            .unwrap();
        store::role_policies::set(&f.store.pool, f.role, &value, previous.revision)
            .await
            .unwrap();
        assert!(matches!(f.read().await, Err(AccessError::InvalidAuthority)));
    }
    let previous = store::role_policies::get(&f.store.pool, f.role)
        .await
        .unwrap()
        .unwrap();
    store::role_policies::clear(&f.store.pool, f.role, previous.revision)
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::MissingAuthority)));
}

#[tokio::test]
async fn access_boundary_corrupt_stored_policy_and_filter_fail_closed() {
    let f = Fixture::new().await;
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE role_policy SET policy = '{invalid', revision = revision + 1 WHERE role_id = ?",
    )
    .bind(f.role)
    .execute(&f.store.pool)
    .await
    .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(f.read().await.is_err());
    sqlx::query("UPDATE role_policy SET policy = ?, revision = revision + 1 WHERE role_id = ?")
        .bind(serde_json::to_string(&policy()).unwrap())
        .bind(f.role)
        .execute(&f.store.pool)
        .await
        .unwrap();
    store::read_filter::set(&f.store.pool, &f.person, Some("not JSON"))
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::InvalidAuthority)));
}

#[tokio::test]
async fn access_boundary_person_filter_only_narrows_and_refreshes_without_new_login() {
    let f = Fixture::new().await;
    let another = record(&f.store, RecordKind::Plain).await;
    let mut selected = policy();
    selected.read = Predicate::UidEq(f.record.clone());
    f.set_policy(selected).await;
    store::read_filter::set(
        &f.store.pool,
        &f.person,
        Some(&serde_json::to_string(&all()).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(
        f.read().await.unwrap().records,
        BTreeSet::from([f.record.clone()])
    );
    store::read_filter::set(
        &f.store.pool,
        &f.person,
        Some(&serde_json::to_string(&Predicate::UidEq(another)).unwrap()),
    )
    .await
    .unwrap();
    assert!(f.read().await.unwrap().records.is_empty());
    store::read_filter::set(&f.store.pool, &f.person, None)
        .await
        .unwrap();
    assert!(f.read().await.unwrap().records.contains(&f.record));
    f.revoke_permission("read").await;
    assert!(matches!(
        f.read().await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
}

#[tokio::test]
async fn access_boundary_grant_epoch_and_device_revocation_invalidate_captured_access() {
    let f = Fixture::new().await;
    store::logins::revoke(&f.store.pool, &f.peer).await.unwrap();
    store::logins::grant(&f.store.pool, &f.peer, &f.person)
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::InvalidSession)));
    let f = Fixture::new().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let device = f.admission.device();
    let revoked = session_access::compare_and_set_revoked_on(
        &mut tx,
        &f.person,
        &device.node_id,
        device.revision,
        true,
    )
    .await
    .unwrap();
    session_access::compare_and_set_revoked_on(
        &mut tx,
        &f.person,
        &device.node_id,
        revoked.revision,
        false,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(f.read().await, Err(AccessError::InvalidSession)));
}

#[tokio::test]
async fn access_boundary_person_disable_reactivate_does_not_revive_admission() {
    let f = Fixture::new().await;
    store::people::deactivate(&f.store.pool, &f.person, "now", None)
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::InvalidSession)));
    store::people::reactivate(&f.store.pool, &f.person)
        .await
        .unwrap();
    assert!(matches!(f.read().await, Err(AccessError::InvalidSession)));
}

#[tokio::test]
async fn access_boundary_positive_allowlist_replaces_local_default_and_hidden_wins() {
    let f = Fixture::new().await;
    visibility(&f, &f.record, "actor", Some(&f.other), "visible", None).await;
    assert!(!f.read().await.unwrap().records.contains(&f.record));
    visibility(
        &f,
        &f.record,
        "role",
        Some(&f.role.to_string()),
        "visible",
        None,
    )
    .await;
    assert!(f.read().await.unwrap().records.contains(&f.record));
    visibility(&f, &f.record, "actor", Some(&f.person), "hidden", None).await;
    visibility(&f, &f.record, "public", None, "visible", None).await;
    assert!(!f.read().await.unwrap().records.contains(&f.record));
}

#[tokio::test]
async fn access_boundary_organ_actor_is_not_employee_and_both_real_organ_scopes_match() {
    let f = Fixture::new().await;
    visibility(&f, &f.record, "actor", Some(&f.hosted), "visible", None).await;
    assert!(!f.read().await.unwrap().records.contains(&f.record));
    visibility(&f, &f.record, "organ", Some(&f.peer), "visible", None).await;
    assert!(f.read().await.unwrap().records.contains(&f.record));
    visibility(&f, &f.record, "organ", Some(&f.hosted), "hidden", None).await;
    assert!(!f.read().await.unwrap().records.contains(&f.record));
    let second = record(&f.store, RecordKind::Plain).await;
    visibility(&f, &second, "organ", Some(&f.hosted), "visible", None).await;
    visibility(&f, &second, "organ", Some(&f.peer), "hidden", None).await;
    assert!(!f.read().await.unwrap().records.contains(&second));
}

#[tokio::test]
async fn access_boundary_unknown_and_field_rules_refuse_only_affected_targets() {
    let f = Fixture::new().await;
    let field = record(&f.store, RecordKind::Plain).await;
    let safe = record(&f.store, RecordKind::Plain).await;
    visibility(&f, &f.record, "future-subject", None, "visible", None).await;
    visibility(&f, &field, "actor", Some(&f.other), "hidden", Some("body")).await;
    let readable = f.read().await.unwrap().records;
    assert!(!readable.contains(&f.record));
    assert!(!readable.contains(&field));
    assert!(readable.contains(&safe));
}

#[tokio::test]
async fn access_boundary_former_fact_author_cannot_read_foreign_or_policy_excluded_record() {
    let f = Fixture::new().await;
    sqlx::query("UPDATE record SET organ_uid = ? WHERE uid = ?")
        .bind(&f.peer)
        .bind(&f.record)
        .execute(&f.store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut new =
        nucleus::NewFact::quantity(&f.record, store::exact::one(), nucleus::Cause::user_edit());
    new.actor_uid = Some(f.person.clone());
    let previous = store::facts::last_hash(&mut tx).await.unwrap();
    let fact = nucleus::fact::seal(new, &previous, chrono::Utc::now());
    store::facts::insert(&mut tx, &fact).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        store::visibility::visible_targets(&f.store.pool, &f.person)
            .await
            .unwrap()
            .contains(&f.record)
    );
    assert!(!f.read().await.unwrap().records.contains(&f.record));
    visibility(&f, &f.record, "actor", Some(&f.person), "visible", None).await;
    assert!(f.read().await.unwrap().records.contains(&f.record));
    let mut narrowed = policy();
    narrowed.read = Predicate::Not(Box::new(Predicate::UidEq(f.record.clone())));
    f.set_policy(narrowed).await;
    assert!(!f.read().await.unwrap().records.contains(&f.record));
}

#[tokio::test]
async fn access_boundary_whole_root_admits_foreign_members_but_child_grant_never_overrides_root() {
    let f = Fixture::new().await;
    let root = record(&f.store, RecordKind::Rule).await;
    let member = record(&f.store, RecordKind::Plain).await;
    sqlx::query("UPDATE record SET replica_root = ?, organ_uid = ? WHERE uid = ?")
        .bind(&root)
        .bind(&f.peer)
        .bind(&member)
        .execute(&f.store.pool)
        .await
        .unwrap();
    visibility(&f, &member, "public", None, "visible", None).await;
    assert!(!f.read().await.unwrap().records.contains(&member));
    visibility(&f, &root, "actor", Some(&f.person), "visible", None).await;
    assert!(f.read().await.unwrap().records.contains(&member));
    visibility(&f, &root, "organ", Some(&f.peer), "hidden", None).await;
    assert!(!f.read().await.unwrap().records.contains(&member));
}

#[tokio::test]
async fn access_boundary_root_scope_preserves_member_allowlist_and_independent_role_selection() {
    let f = Fixture::new().await;
    let root = record(&f.store, RecordKind::Rule).await;
    let member = record(&f.store, RecordKind::Plain).await;
    sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
        .bind(&root)
        .bind(&member)
        .execute(&f.store.pool)
        .await
        .unwrap();
    visibility(&f, &root, "actor", Some(&f.person), "visible", None).await;
    visibility(&f, &member, "actor", Some(&f.other), "visible", None).await;
    assert!(!f.read().await.unwrap().records.contains(&member));
    visibility(
        &f,
        &member,
        "role",
        Some(&f.role.to_string()),
        "visible",
        None,
    )
    .await;
    let mut selected = policy();
    selected.read = Predicate::KindEq("plain".into());
    f.set_policy(selected).await;
    let readable = f.read().await.unwrap().records;
    assert!(readable.contains(&member));
    assert!(!readable.contains(&root));
}

#[tokio::test]
async fn access_boundary_nested_cyclic_and_deleted_roots_refuse_members() {
    let f = Fixture::new().await;
    let root = record(&f.store, RecordKind::Plain).await;
    let other_root = record(&f.store, RecordKind::Plain).await;
    sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
        .bind(&root)
        .bind(&f.record)
        .execute(&f.store.pool)
        .await
        .unwrap();
    visibility(&f, &root, "actor", Some(&f.person), "visible", None).await;
    for parent in [&other_root, &f.record, &root] {
        sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
            .bind(parent)
            .bind(&root)
            .execute(&f.store.pool)
            .await
            .unwrap();
        assert_eq!(
            f.read().await.unwrap().records.contains(&f.record),
            parent == &root
        );
    }
    store::records::mark_deleted(&f.store.pool, &root)
        .await
        .unwrap();
    assert!(!f.read().await.unwrap().records.contains(&f.record));
}

#[tokio::test]
async fn access_boundary_drafts_require_exact_person_operator_not_author_or_company() {
    let f = Fixture::new().await;
    let draft = record(&f.store, RecordKind::MessageDraft).await;
    for (author, operator, visible) in [
        (&f.other, &f.person, true),
        (&f.person, &f.other, false),
        (&f.person, &f.hosted, false),
    ] {
        store::records::set_extension(
            &f.store.pool,
            &draft,
            "lince.message-draft",
            &json!({"author": author, "operator": operator, "thread": null}),
        )
        .await
        .unwrap();
        assert_eq!(f.read().await.unwrap().records.contains(&draft), visible);
    }
}

#[tokio::test]
async fn access_boundary_vocabulary_adoption_admits_concepts_not_foreign_records_or_hidden_parents()
{
    let f = Fixture::new().await;
    let foreign_lingua =
        store::linguas::create(&f.store.pool, "Foreign private", Some(&f.peer), "private")
            .await
            .unwrap();
    let parent = store::concepts::create_in(&f.store.pool, &foreign_lingua, "Hidden parent", &[])
        .await
        .unwrap();
    let child = store::concepts::create(&f.store.pool, "Local child", &[&parent])
        .await
        .unwrap();
    let public_lingua =
        store::linguas::create(&f.store.pool, "Explicit public", Some(&f.peer), "public")
            .await
            .unwrap();
    let public = store::concepts::create_in(&f.store.pool, &public_lingua, "Public concept", &[])
        .await
        .unwrap();
    let shared_lingua =
        store::linguas::create(&f.store.pool, "Foreign shared", Some(&f.peer), "shared")
            .await
            .unwrap();
    let shared =
        store::concepts::create_in(&f.store.pool, &shared_lingua, "Shared foreign concept", &[])
            .await
            .unwrap();
    let sets = f.read().await.unwrap();
    assert!(sets.concepts.contains(&child));
    assert!(sets.concepts.contains(&public));
    assert!(!sets.concepts.contains(&parent));
    assert!(!sets.concepts.contains(&shared));
    assert!(
        !sets
            .concept_parents
            .contains(&(child.clone(), parent.clone()))
    );
    assert!(sets.linguas.contains("g_local"));
    store::linguas::adopt(&f.store.pool, "g_local", &parent)
        .await
        .unwrap();
    let sets = f.read().await.unwrap();
    assert!(sets.concepts.contains(&parent));
    assert!(
        sets.concept_parents
            .contains(&(child.clone(), parent.clone()))
    );
    assert!(!sets.records.contains(&f.peer));
    visibility(&f, &parent, "actor", Some(&f.other), "visible", None).await;
    assert!(!f.read().await.unwrap().concepts.contains(&parent));
    visibility(&f, &shared, "actor", Some(&f.person), "visible", None).await;
    assert!(f.read().await.unwrap().concepts.contains(&shared));
}

#[tokio::test]
async fn access_boundary_assertion_reference_sets_hide_subject_object_predicate_and_unit() {
    let f = Fixture::new().await;
    let predicate = store::concepts::create(&f.store.pool, "Relation", &[])
        .await
        .unwrap();
    let hidden_predicate = store::concepts::create(&f.store.pool, "Hidden relation", &[])
        .await
        .unwrap();
    let unit = store::concepts::create(&f.store.pool, "Hidden unit", &[])
        .await
        .unwrap();
    let hidden = record(&f.store, RecordKind::Plain).await;
    visibility(&f, &hidden, "actor", Some(&f.person), "hidden", None).await;
    visibility(
        &f,
        &hidden_predicate,
        "actor",
        Some(&f.person),
        "hidden",
        None,
    )
    .await;
    visibility(&f, &unit, "actor", Some(&f.person), "hidden", None).await;
    let safe = assertion(&f, &f.record, &predicate, None, None).await;
    let object = assertion(&f, &f.record, &predicate, Some(&hidden), None).await;
    let subject = assertion(&f, &hidden, &predicate, None, None).await;
    let predicate_hidden = assertion(&f, &f.record, &hidden_predicate, None, None).await;
    let measured = assertion(&f, &f.record, &predicate, Some(&f.other), Some(&unit)).await;
    let sets = f.read().await.unwrap();
    assert!(sets.assertions.contains(&safe));
    for hidden in [object, subject, predicate_hidden, measured] {
        assert!(!sets.assertions.contains(&hidden));
    }
    assert!(sets.records.contains(&f.record));
}

#[tokio::test]
async fn access_boundary_hidden_policy_operand_is_valid_but_missing_not_operand_errors() {
    let f = Fixture::new().await;
    let hidden = record(&f.store, RecordKind::Plain).await;
    visibility(&f, &hidden, "actor", Some(&f.person), "hidden", None).await;
    let mut selected = policy();
    selected.read = Predicate::Not(Box::new(Predicate::UidEq(hidden.clone())));
    f.set_policy(selected).await;
    let sets = f.read().await.unwrap();
    assert!(sets.records.contains(&f.record));
    assert!(!sets.records.contains(&hidden));
    let mut invalid = policy();
    invalid.read = Predicate::Not(Box::new(Predicate::UidEq(nucleus::new_uid("r"))));
    f.set_policy(invalid).await;
    assert!(matches!(
        f.read().await,
        Err(AccessError::Policy(AuthorityError::MissingDependency))
    ));
    let mut invalid = policy();
    invalid.read = Predicate::Any(vec![all(), Predicate::SlugEq("not-authority".into())]);
    f.set_policy(invalid).await;
    assert!(matches!(
        f.read().await,
        Err(AccessError::Policy(AuthorityError::UnsupportedPredicate))
    ));
}

#[tokio::test]
async fn access_boundary_actual_staged_body_change_returns_final_state_before_commit() {
    let f = Fixture::new().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Accepted candidate").await;
    let decision = access
        .finish_changes(&[target(&f.record, &[])])
        .await
        .unwrap();
    assert_eq!(
        decision.records[&f.record].properties,
        BTreeSet::from([Property::Body])
    );
    assert_eq!(
        decision.target_content(&f.record).unwrap().body,
        "Accepted candidate"
    );
    assert!(decision.revision(&f.record).unwrap() > 1);
    let staged: String = sqlx::query_scalar("SELECT body FROM record WHERE uid = ?")
        .bind(&f.record)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(staged, "Accepted candidate");
    tx.rollback().await.unwrap();
    assert_eq!(
        store::records::get(&f.store.pool, &f.record)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Initial body"
    );
}

#[tokio::test]
async fn access_boundary_changed_write_outside_declared_targets_refuses_and_rolls_back() {
    let f = Fixture::new().await;
    let hidden_change = record(&f.store, RecordKind::Plain).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Intended").await;
    body(&mut access, &hidden_change, "Unlisted").await;
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::InvalidMutation)
    ));
    tx.rollback().await.unwrap();
    for uid in [&f.record, &hidden_change] {
        assert_eq!(
            store::records::get(&f.store.pool, uid)
                .await
                .unwrap()
                .unwrap()
                .body,
            "Initial body"
        );
    }
}

#[tokio::test]
async fn access_boundary_create_absent_uid_checks_complete_state_and_hosted_origin() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[uid.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO record (uid, kind, head, body, quantity_mantissa, quantity_scale, organ_uid, created_at, updated_at)
                 VALUES (?, 'plain', 'New', 'New body', '0', 0, ?, 'now', 'now')")
        .bind(&uid).bind(&f.hosted).execute(&mut **access.transaction_for_staging()).await.unwrap();
    let decision = access.finish_changes(&[target(&uid, &[])]).await.unwrap();
    assert_eq!(decision.records[&uid].operation, Operation::Create);
    assert!(decision.records[&uid].properties.contains(&Property::Organ));
    assert_eq!(
        decision.target_content(&uid).unwrap().organ_uid.as_deref(),
        Some(f.hosted.as_str())
    );
    tx.commit().await.unwrap();
    assert!(f.read().await.unwrap().records.contains(&uid));
}

#[tokio::test]
async fn access_boundary_creation_cannot_add_excluded_assertion_or_borrow_another_grant() {
    let f = Fixture::new().await;
    let excluded = store::concepts::create(&f.store.pool, "Owner-selected-sensitive", &[])
        .await
        .unwrap();
    let mut selected = policy();
    selected.read = Predicate::Not(Box::new(Predicate::ConceptIn(excluded.clone())));
    for grant in &mut selected.grants {
        grant.assertions_add.push(AssertionGrant {
            predicate_uid: excluded.clone(),
            target: AssertionTarget::Unary,
            role: AssertionRole::Ordinary,
            properties: BTreeSet::new(),
        });
    }
    f.set_policy(selected).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO record_assertion (uid, subject_uid, predicate_uid, role, created_at) VALUES (?, ?, ?, 'ordinary', 'now')")
        .bind(nucleus::new_uid("a")).bind(&f.record).bind(&excluded)
        .execute(&mut **access.transaction_for_staging()).await.unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    let mut divided = policy();
    divided
        .grants
        .retain(|grant| grant.operation == Operation::Update);
    divided.grants[0].properties = BTreeSet::from([Property::Body]);
    let mut head = divided.grants[0].clone();
    head.properties = BTreeSet::from([Property::Head]);
    divided.grants.push(head);
    f.set_policy(divided).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Changed body").await;
    assert!(matches!(
        access
            .finish_changes(&[target(&f.record, &[Property::Head])])
            .await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_canceled_or_duplicate_text_requires_truthful_touched_property() {
    let f = Fixture::new().await;
    for footprint in [Vec::new(), vec![Property::Head]] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = access::prepare_changes_on(
            &mut tx,
            &f.admission,
            &f.hosted,
            &[f.record.clone()],
            AccessLimits::default(),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE record SET head = head WHERE uid = ?")
            .bind(&f.record)
            .execute(&mut **access.transaction_for_staging())
            .await
            .unwrap();
        let decision = access
            .finish_changes(&[target(&f.record, &footprint)])
            .await;
        assert_eq!(decision.is_ok(), !footprint.is_empty());
        tx.rollback().await.unwrap();
    }
    let mut readonly = policy();
    readonly.grants.iter_mut().for_each(|grant| {
        grant.properties.remove(&Property::Head);
    });
    f.set_policy(readonly).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    assert!(matches!(
        access
            .finish_changes(&[target(&f.record, &[Property::Head])])
            .await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_control_and_assignment_changes_during_staging_refuse() {
    let f = Fixture::new().await;
    for change in 0..4 {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = access::prepare_changes_on(
            &mut tx,
            &f.admission,
            &f.hosted,
            &[f.record.clone()],
            AccessLimits::default(),
        )
        .await
        .unwrap();
        body(&mut access, &f.record, "Candidate").await;
        let transaction = access.transaction_for_staging();
        match change {
            0 => {
                sqlx::query("UPDATE person_access SET revision = revision + 1, role_id = NULL WHERE person_uid = ?")
                .bind(&f.person).execute(&mut **transaction).await.unwrap();
            }
            1 => {
                sqlx::query("UPDATE role_policy SET revision = revision + 1 WHERE role_id = ?")
                    .bind(f.role)
                    .execute(&mut **transaction)
                    .await
                    .unwrap();
            }
            2 => {
                sqlx::query("UPDATE record SET replica_root = uid WHERE uid = ?")
                    .bind(&f.record)
                    .execute(&mut **transaction)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query("INSERT INTO visibility_rule (uid, subject_kind, target_uid, grant_level) VALUES (?, 'public', ?, 'visible')")
                .bind(nucleus::new_uid("v")).bind(&f.record).execute(&mut **transaction).await.unwrap();
            }
        }
        assert!(
            access
                .finish_changes(&[target(&f.record, &[])])
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn access_boundary_empty_namespace_and_version_only_changes_are_not_empty_property_edits() {
    let f = Fixture::new().await;
    store::records::set_extension(
        &f.store.pool,
        &f.record,
        "example.fields",
        &json!({"title": "same"}),
    )
    .await
    .unwrap();
    let mut extended = policy();
    for grant in &mut extended.grants {
        grant
            .properties
            .insert(Property::Extension(ExtensionProperty {
                namespace: "example.fields".into(),
                field: "title".into(),
            }));
    }
    f.set_policy(extended).await;
    for query in [
        "INSERT INTO record_extension (record_uid, namespace, version, fds) VALUES (?, 'empty.fields', 1, '{}')",
        "UPDATE record_extension SET version = version + 1 WHERE record_uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = access::prepare_changes_on(
            &mut tx,
            &f.admission,
            &f.hosted,
            &[f.record.clone()],
            AccessLimits::default(),
        )
        .await
        .unwrap();
        sqlx::query(query)
            .bind(&f.record)
            .execute(&mut **access.transaction_for_staging())
            .await
            .unwrap();
        assert!(matches!(
            access.finish_changes(&[target(&f.record, &[])]).await,
            Err(AccessError::InvalidMutation)
        ));
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn access_boundary_unit_and_place_changes_require_reference_visibility() {
    let f = Fixture::new().await;
    let unit = store::concepts::create(&f.store.pool, "Restricted unit", &[])
        .await
        .unwrap();
    let place = store::places::create(&f.store.pool, 1.0, 2.0, None)
        .await
        .unwrap();
    assert!(f.read().await.unwrap().places.contains(&place));
    visibility(&f, &unit, "actor", Some(&f.other), "visible", None).await;
    visibility(&f, &place, "actor", Some(&f.other), "visible", None).await;
    assert!(!f.read().await.unwrap().places.contains(&place));
    for (query, reference) in [
        ("UPDATE record SET unit_uid = ? WHERE uid = ?", &unit),
        ("UPDATE record SET place_uid = ? WHERE uid = ?", &place),
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        let mut access = access::prepare_changes_on(
            &mut tx,
            &f.admission,
            &f.hosted,
            &[f.record.clone()],
            AccessLimits::default(),
        )
        .await
        .unwrap();
        sqlx::query(query)
            .bind(reference)
            .bind(&f.record)
            .execute(&mut **access.transaction_for_staging())
            .await
            .unwrap();
        assert!(matches!(
            access.finish_changes(&[target(&f.record, &[])]).await,
            Err(AccessError::Policy(AuthorityError::Denied))
        ));
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn access_boundary_delete_own_is_not_delete_and_restore_requires_update() {
    let f = Fixture::new().await;
    f.revoke_permission("delete").await;
    let permission = store::auth::ensure_permission(&f.store.pool, "record", "delete_own")
        .await
        .unwrap();
    store::auth::grant(&f.store.pool, f.role, permission)
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE record SET deleted_at = 'now' WHERE uid = ?")
        .bind(&f.record)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    store::records::mark_deleted(&f.store.pool, &f.record)
        .await
        .unwrap();
    assert!(!f.read().await.unwrap().records.contains(&f.record));
    f.revoke_permission("update").await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&f.record)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_limits_missing_scope_and_incomplete_batch_refuse() {
    let f = Fixture::new().await;
    for scope in ["invalid".into(), nucleus::new_uid("r"), f.person.clone()] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            access::load_on(&mut tx, &f.admission, &scope, &[], AccessLimits::default())
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    for limits in [
        AccessLimits {
            snapshot: store::access_snapshot::AccessSnapshotLimits {
                records: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        AccessLimits {
            authority: protein::authority::Limits {
                steps: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        assert!(
            access::load_on(&mut tx, &f.admission, &f.hosted, &[], limits)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone(), f.other.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::InvalidMutation)
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_complete_classified_creation_can_be_authorized_without_blind_writes() {
    let f = Fixture::new().await;
    let classification = store::concepts::create(&f.store.pool, "Chosen ordinary category", &[])
        .await
        .unwrap();
    let mut selected = policy();
    selected.read = Predicate::Any(vec![
        Predicate::KindEq("organ".into()),
        Predicate::ConceptIn(classification.clone()),
    ]);
    for grant in &mut selected.grants {
        grant.selector = Predicate::ConceptIn(classification.clone());
        grant.assertions_add.push(AssertionGrant {
            predicate_uid: classification.clone(),
            target: AssertionTarget::Unary,
            role: AssertionRole::Ordinary,
            properties: BTreeSet::new(),
        });
    }
    f.set_policy(selected).await;
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[uid.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    let transaction = access.transaction_for_staging();
    sqlx::query("INSERT INTO record (uid, kind, head, body, quantity_mantissa, quantity_scale, organ_uid, created_at, updated_at)
                 VALUES (?, 'plain', 'Classified creation', 'Complete candidate', '0', 0, ?, 'now', 'now')")
        .bind(&uid).bind(&f.hosted).execute(&mut **transaction).await.unwrap();
    let assertion_uid = nucleus::new_uid("a");
    sqlx::query("INSERT INTO record_assertion (uid, subject_uid, predicate_uid, role, created_at) VALUES (?, ?, ?, 'ordinary', 'now')")
        .bind(&assertion_uid).bind(&uid).bind(&classification).execute(&mut **transaction).await.unwrap();
    let decision = access.finish_changes(&[target(&uid, &[])]).await.unwrap();
    assert!(decision.readable.records.contains(&uid));
    assert_eq!(
        decision.records[&uid].assertions_added,
        BTreeSet::from([assertion_uid])
    );
    tx.rollback().await.unwrap();
    assert!(
        store::records::get(&f.store.pool, &uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn access_boundary_removing_exclusion_from_hidden_current_record_never_becomes_a_grant() {
    let f = Fixture::new().await;
    let excluded = store::concepts::create(&f.store.pool, "Restricted category", &[])
        .await
        .unwrap();
    let assertion_uid = assertion(&f, &f.record, &excluded, None, None).await;
    let mut selected = policy();
    selected.read = Predicate::Not(Box::new(Predicate::ConceptIn(excluded.clone())));
    for grant in &mut selected.grants {
        grant.assertions_remove.push(AssertionGrant {
            predicate_uid: excluded.clone(),
            target: AssertionTarget::Unary,
            role: AssertionRole::Ordinary,
            properties: BTreeSet::new(),
        });
    }
    f.set_policy(selected).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    assert!(access.readable_target(&f.record).is_none());
    sqlx::query("UPDATE record_assertion SET retracted_at = 'now' WHERE uid = ?")
        .bind(&assertion_uid)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_batch_reads_actual_full_state_for_every_target() {
    let f = Fixture::new().await;
    let second = record(&f.store, RecordKind::Plain).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone(), second.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "First candidate").await;
    body(&mut access, &second, "Second candidate").await;
    let result = access
        .finish_changes(&[target(&f.record, &[]), target(&second, &[])])
        .await
        .unwrap();
    assert_eq!(result.records.len(), 2);
    assert_eq!(
        result.target_content(&f.record).unwrap().body,
        "First candidate"
    );
    assert_eq!(
        result.target_content(&second).unwrap().body,
        "Second candidate"
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn access_boundary_staged_device_revocation_is_revalidated_before_decision() {
    let f = Fixture::new().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Must not commit").await;
    session_access::compare_and_set_revoked_on(
        access.transaction_for_staging(),
        &f.person,
        &f.admission.device().node_id,
        f.admission.device().revision,
        true,
    )
    .await
    .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::InvalidSession)
    ));
    tx.rollback().await.unwrap();
    assert!(f.read().await.unwrap().records.contains(&f.record));
}

#[tokio::test]
async fn access_boundary_unknown_password_peer_has_no_contact_organ_scope() {
    let f = Fixture::new().await;
    store::auth::create_credential(
        &f.store.pool,
        &f.person,
        "password-person",
        "opaque-test-hash",
        f.role,
    )
    .await
    .unwrap();
    let unknown = format!("{:064x}", 999);
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let auth = session_access::password_on(&mut tx, "password-person")
        .await
        .unwrap()
        .unwrap()
        .authentication()
        .clone();
    let admission = session_access::register_device_on(&mut tx, &auth, &unknown)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    visibility(&f, &f.record, "organ", Some(&f.peer), "visible", None).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let access = access::load_on(&mut tx, &admission, &f.hosted, &[], AccessLimits::default())
        .await
        .unwrap();
    assert_eq!(access.peer_organ(), None);
    assert!(!access.readable().records.contains(&f.record));
    drop(access);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_known_password_peer_block_invalidates_both_source_and_scope() {
    let f = Fixture::new().await;
    store::auth::create_credential(
        &f.store.pool,
        &f.person,
        "password-person",
        "opaque-test-hash",
        f.role,
    )
    .await
    .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let auth = session_access::password_on(&mut tx, "password-person")
        .await
        .unwrap()
        .unwrap()
        .authentication()
        .clone();
    let admission =
        session_access::register_device_on(&mut tx, &auth, &f.admission.device().node_id)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    store::organs::set_trust(&f.store.pool, &f.peer, "blocked")
        .await
        .unwrap();
    store::organs::set_trust(&f.store.pool, &f.peer, "unknown")
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(matches!(
        access::load_on(&mut tx, &admission, &f.hosted, &[], AccessLimits::default()).await,
        Err(AccessError::InvalidSession)
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_draft_changed_linked_author_requires_readable_reference() {
    let f = Fixture::new().await;
    let draft = record(&f.store, RecordKind::MessageDraft).await;
    store::records::set_extension(
        &f.store.pool,
        &draft,
        "lince.message-draft",
        &json!({"author": f.person, "operator": f.person}),
    )
    .await
    .unwrap();
    visibility(&f, &f.other, "actor", Some(&f.person), "hidden", None).await;
    let mut extended = policy();
    for grant in &mut extended.grants {
        grant
            .properties
            .insert(Property::Extension(ExtensionProperty {
                namespace: "lince.message-draft".into(),
                field: "author".into(),
            }));
    }
    f.set_policy(extended).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[draft.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE record_extension SET fds = ?, version = version + 1 WHERE record_uid = ? AND namespace = 'lince.message-draft'")
        .bind(json!({"author": f.other, "operator": f.person}).to_string()).bind(&draft)
        .execute(&mut **access.transaction_for_staging()).await.unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&draft, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_declared_creation_without_actual_row_is_incomplete() {
    let f = Fixture::new().await;
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[uid.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    assert!(access.finish_changes(&[target(&uid, &[])]).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_disconnected_under_cycle_refuses_without_pruning_dependencies() {
    let f = Fixture::new().await;
    let relation = store::concepts::create(&f.store.pool, "Owner traversal", &[])
        .await
        .unwrap();
    let left = record(&f.store, RecordKind::Plain).await;
    let right = record(&f.store, RecordKind::Plain).await;
    assertion(&f, &left, &relation, Some(&right), None).await;
    assertion(&f, &right, &relation, Some(&left), None).await;
    let mut selected = policy();
    selected.read = Predicate::Under {
        record: f.record.clone(),
        kind: relation,
        include_self: true,
    };
    f.set_policy(selected).await;
    assert!(matches!(
        f.read().await,
        Err(AccessError::Policy(AuthorityError::CyclicGraph))
    ));
}

#[tokio::test]
async fn access_boundary_staging_and_decision_do_not_publish_to_second_connection() {
    let path =
        std::env::temp_dir().join(format!("access-boundary-{}.sqlite", nucleus::new_uid("r")));
    let f = Fixture::with_store(
        Store::open(&format!("sqlite://{}", path.display()))
            .await
            .unwrap(),
    )
    .await;
    let mut observer = f.store.pool.acquire().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Not public until commit").await;
    let observed: String = sqlx::query_scalar("SELECT body FROM record WHERE uid = ?")
        .bind(&f.record)
        .fetch_one(&mut *observer)
        .await
        .unwrap();
    assert_eq!(observed, "Initial body");
    access
        .finish_changes(&[target(&f.record, &[])])
        .await
        .unwrap();
    let observed: String = sqlx::query_scalar("SELECT body FROM record WHERE uid = ?")
        .bind(&f.record)
        .fetch_one(&mut *observer)
        .await
        .unwrap();
    assert_eq!(observed, "Initial body");
    tx.commit().await.unwrap();
    let observed: String = sqlx::query_scalar("SELECT body FROM record WHERE uid = ?")
        .bind(&f.record)
        .fetch_one(&mut *observer)
        .await
        .unwrap();
    assert_eq!(observed, "Not public until commit");
    drop(observer);
    f.store.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn access_boundary_deleted_self_root_restore_needs_whole_root_grant_without_child_bypass() {
    let f = Fixture::new().await;
    let child = record(&f.store, RecordKind::Plain).await;
    sqlx::query("UPDATE record SET replica_root = uid, deleted_at = 'deleted' WHERE uid = ?")
        .bind(&f.record)
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
        .bind(&f.record)
        .bind(&child)
        .execute(&f.store.pool)
        .await
        .unwrap();
    visibility(&f, &child, "public", None, "visible", None).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&f.record)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    visibility(&f, &f.record, "actor", Some(&f.person), "visible", None).await;
    let sets = f.read().await.unwrap();
    assert!(!sets.records.contains(&f.record));
    assert!(!sets.records.contains(&child));
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[child.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &child, "Child bypass attempt").await;
    assert!(matches!(
        access.finish_changes(&[target(&child, &[])]).await,
        Err(AccessError::Policy(AuthorityError::Denied))
    ));
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&f.record)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    let decision = access
        .finish_changes(&[target(&f.record, &[])])
        .await
        .unwrap();
    assert_eq!(decision.records[&f.record].operation, Operation::Restore);
    assert!(decision.readable.records.contains(&child));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn access_boundary_concept_origin_control_cannot_hide_in_graph_projection() {
    let f = Fixture::new().await;
    let concept = store::concepts::create(&f.store.pool, "Ordinary vocabulary", &[])
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let mut access = access::prepare_changes_on(
        &mut tx,
        &f.admission,
        &f.hosted,
        &[f.record.clone()],
        AccessLimits::default(),
    )
    .await
    .unwrap();
    body(&mut access, &f.record, "Candidate").await;
    sqlx::query("UPDATE concept SET origin_organ = ? WHERE uid = ?")
        .bind(&f.peer)
        .bind(&concept)
        .execute(&mut **access.transaction_for_staging())
        .await
        .unwrap();
    assert!(matches!(
        access.finish_changes(&[target(&f.record, &[])]).await,
        Err(AccessError::AuthorityChanged)
    ));
    tx.rollback().await.unwrap();
}
