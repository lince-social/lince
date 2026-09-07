use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::karma::{
    Capability, CapabilitySet, DelegationGrantSchema, DelegationGrantSpec, GrantAuthorityDenial,
    GrantAuthorityRequest, GrantProgramRevisionScope, GrantStatus, GrantTargetScope,
    GrantTemplateScope, ProgramAst, ProgramSchema, ReferenceKind, Slug, TimestampMs, TypedUid,
};
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;

const PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAV";
const OTHER_PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAW";

#[tokio::test]
async fn typed_grant_actions_walk_the_lifecycle_and_hold_cas() {
    let engine = signed_engine().await;
    let program_uid = program(&engine).await;

    let create = Action::CreateKarmaGrant {
        request_id: "action-create-grant".to_string(),
        slug: slug("action.grant"),
        grant: spec(&program_uid, two_capabilities()),
    };
    let created = engine.act_at(create.clone(), None, now(0)).await.unwrap();
    let grant_uid = created.created.clone().unwrap();
    assert_eq!(created.facts.len(), 1);
    assert_eq!(created.facts[0].actor_uid.as_deref(), Some(PERSON_UID));

    let replay = engine.act_at(create, None, now(0)).await.unwrap();
    assert_eq!(replay.created.as_deref(), Some(grant_uid.as_str()));
    assert!(
        replay.facts.is_empty(),
        "a replayed grant Action must not republish its Fact"
    );

    let handle = engine.get_karma_grant(&grant_uid).await.unwrap().unwrap();
    assert_eq!(handle.status, GrantStatus::Draft);
    assert_eq!(handle.principal_person_uid, PERSON_UID);

    let stale = engine
        .act_at(
            Action::ActivateKarmaGrant {
                request_id: "action-activate-grant-stale".to_string(),
                grant_uid: grant_uid.clone(),
                expected_handle_revision: 9,
                revision_hash: handle.head_revision_hash.clone(),
            },
            None,
            now(1),
        )
        .await
        .unwrap_err();
    assert!(
        format!("{stale:?}").contains("karma_stale_handle_revision"),
        "expected a stale handle conflict, got {stale:?}"
    );

    engine
        .act_at(
            Action::ActivateKarmaGrant {
                request_id: "action-activate-grant".to_string(),
                grant_uid: grant_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: handle.head_revision_hash.clone(),
            },
            None,
            now(2),
        )
        .await
        .unwrap();
    let evaluation = engine
        .evaluate_karma_grant(&grant_uid, &request(&program_uid))
        .await
        .unwrap();
    assert!(evaluation.decision.allowed);

    engine
        .act_at(
            Action::NarrowKarmaGrant {
                request_id: "action-narrow-grant".to_string(),
                grant_uid: grant_uid.clone(),
                expected_handle_revision: 2,
                grant: spec(&program_uid, one_capability()),
            },
            None,
            now(3),
        )
        .await
        .unwrap();
    let mut dropped = request(&program_uid);
    dropped.capability = Capability::LinkCreate;
    assert_eq!(
        engine
            .evaluate_karma_grant(&grant_uid, &dropped)
            .await
            .unwrap()
            .decision
            .denials,
        vec![GrantAuthorityDenial::CapabilityMissing]
    );

    engine
        .act_at(
            Action::RevokeKarmaGrant {
                request_id: "action-revoke-grant".to_string(),
                grant_uid: grant_uid.clone(),
                expected_handle_revision: 3,
            },
            None,
            now(4),
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_karma_grant(&grant_uid, &request(&program_uid))
            .await
            .unwrap()
            .decision
            .denials,
        vec![GrantAuthorityDenial::GrantRevoked]
    );
}

#[tokio::test]
async fn a_grant_can_only_be_signed_by_its_own_principal() {
    let engine = Engine::open_memory().await.unwrap();
    let program_uid = program(&engine).await;
    let create = || Action::CreateKarmaGrant {
        request_id: format!("action-create-grant-{}", nucleus::new_uid("req")),
        slug: slug("action.principal"),
        grant: spec(&program_uid, two_capabilities()),
    };

    let unsigned = engine.act_at(create(), None, now(0)).await.unwrap_err();
    assert!(
        format!("{unsigned:?}").contains("installed signing key"),
        "expected a missing-signer refusal, got {unsigned:?}"
    );

    engine
        .set_signer(Signer::generate(PERSON_UID, "key:ana"))
        .await
        .unwrap();

    let intruder = privileged_user(&engine, OTHER_PERSON_UID, "intruder").await;
    let borrowed = engine
        .act_at(create(), Some(intruder.to_string()), now(1))
        .await
        .unwrap_err();
    assert!(
        format!("{borrowed:?}").contains("cannot sign for another Person"),
        "expected a principal mismatch refusal, got {borrowed:?}"
    );

    let owner = privileged_user(&engine, PERSON_UID, "owner").await;
    let committed = engine
        .act_at(create(), Some(owner.to_string()), now(2))
        .await
        .unwrap();
    let grant_uid = committed.created.unwrap();
    assert_eq!(
        engine
            .get_karma_grant(&grant_uid)
            .await
            .unwrap()
            .unwrap()
            .principal_person_uid,
        PERSON_UID
    );
}

#[tokio::test]
async fn grant_actions_require_the_karma_permission() {
    let engine = signed_engine().await;
    let program_uid = program(&engine).await;
    person_record(&engine, PERSON_UID, "ana").await;
    let bystander = store::auth::ensure_role(&engine.store.pool, "grant-bystander")
        .await
        .unwrap();
    let user_id = store::auth::create_credential(
        &engine.store.pool,
        PERSON_UID,
        "grant-bystander",
        "hash",
        bystander,
    )
    .await
    .unwrap();

    let denied = engine
        .act_at(
            Action::CreateKarmaGrant {
                request_id: "action-create-grant-denied".to_string(),
                slug: slug("action.denied"),
                grant: spec(&program_uid, two_capabilities()),
            },
            Some(user_id.to_string()),
            now(0),
        )
        .await
        .unwrap_err();
    assert!(
        format!("{denied:?}").contains("karma:create"),
        "expected a permission refusal, got {denied:?}"
    );
}

#[tokio::test]
async fn protein_projects_grants_with_provenance_and_ready_actions() {
    let engine = signed_engine().await;
    let program_uid = program(&engine).await;
    let created = engine
        .act_at(
            Action::CreateKarmaGrant {
                request_id: "action-create-grant-protein".to_string(),
                slug: slug("action.projected"),
                grant: spec(&program_uid, two_capabilities()),
            },
            None,
            now(0),
        )
        .await
        .unwrap();
    let grant_uid = created.created.unwrap();

    let rows = karma_rows(&engine, "grant").await;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["uid"].as_str(), Some(grant_uid.as_str()));
    assert_eq!(row["status"].as_str(), Some("draft"));
    assert_eq!(row["principal_person_uid"].as_str(), Some(PERSON_UID));
    assert_eq!(
        row["signature_provenance"]["key_id"].as_str(),
        Some("key:ana")
    );
    assert_eq!(
        row["signature_provenance"]["signer_person_uid"]["uid"].as_str(),
        Some(PERSON_UID)
    );
    assert_eq!(row["capabilities"]["activate"].as_bool(), Some(true));
    assert_eq!(row["capabilities"]["narrow"].as_bool(), Some(true));
    assert!(row["action_templates"].get("widen").is_none());
    assert_eq!(
        row["action_templates"]["activate"]["action"].as_str(),
        Some("activate-karma-grant")
    );
    assert_eq!(
        row["action_templates"]["activate"]["expected_handle_revision"].as_u64(),
        Some(1)
    );
    assert_eq!(row["authorizes_effects"].as_bool(), Some(false));

    assert!(row["budget"].is_object(), "the budget is projected");
    assert_eq!(
        row["effect_blocking_reasons"][0].as_str(),
        Some("karma_execution_not_implemented")
    );
    assert!(karma_rows(&engine, "intent").await.is_empty());

    let revisions = karma_rows(&engine, "grant_revision").await;
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["grant_uid"].as_str(), Some(grant_uid.as_str()));
    assert!(
        revisions[0]["signature_provenance"]["signature"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
}

#[tokio::test]
async fn authorizing_an_unknown_candidate_creates_no_intent() {
    let engine = signed_engine().await;
    let program_uid = program(&engine).await;
    let created = engine
        .act_at(
            Action::CreateKarmaGrant {
                request_id: "action-create-grant-bridge".to_string(),
                slug: slug("action.bridge"),
                grant: spec(&program_uid, two_capabilities()),
            },
            None,
            now(0),
        )
        .await
        .unwrap();
    let grant_uid = created.created.unwrap();

    let refused = engine
        .act_at(
            Action::RespondKarmaCandidate {
                request_id: "action-authorize-missing".to_string(),
                candidate_hash: nucleus::karma::CanonicalHash::parse(format!(
                    "sha256:{}",
                    "b".repeat(64)
                ))
                .unwrap(),
                expected_state_revision: 1,
                response: nucleus::karma::CandidateReviewAction::Accept,
                authorizing_grant_uid: Some(grant_uid),
            },
            None,
            now(1),
        )
        .await;
    assert!(refused.is_err(), "an unknown candidate authorizes nothing");
    assert!(karma_rows(&engine, "intent").await.is_empty());
}

async fn karma_rows(engine: &Engine, object_kind: &str) -> Vec<Value> {
    protein::execute(
        &engine.store,
        &Protein {
            source: Source::Karma,
            filter: vec![Predicate::KindEq(object_kind.to_string())],
            fields: None,
            include: Include::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        },
    )
    .await
    .unwrap()
}

async fn signed_engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    person_record(&engine, PERSON_UID, "ana").await;
    engine
        .set_signer(Signer::generate(PERSON_UID, "key:ana"))
        .await
        .unwrap();
    engine
}

async fn person_record(engine: &Engine, person_uid: &str, slug: &str) {
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .expect("local organ")
        .uid;
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, 'person', ?, '', '1', 0, ?, '2026-07-24T12:00:00Z', '2026-07-24T12:00:00Z')
         ON CONFLICT(uid) DO NOTHING",
    )
    .bind(person_uid)
    .bind(slug)
    .bind(slug)
    .bind(&organ)
    .execute(&engine.store.pool)
    .await
    .unwrap();
}

async fn privileged_user(engine: &Engine, person_uid: &str, username: &str) -> String {
    person_record(engine, person_uid, username).await;
    let role_id = store::auth::ensure_role(&engine.store.pool, &format!("{username}-role"))
        .await
        .unwrap();
    for (subject, action) in [("karma", "create"), ("karma", "update")] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role_id, permission)
            .await
            .unwrap();
    }
    let user_id =
        store::auth::create_credential(&engine.store.pool, person_uid, username, "hash", role_id)
            .await
            .unwrap();
    user_id
}

async fn program(engine: &Engine) -> String {
    engine
        .act_at(
            Action::CreateKarmaProgram {
                request_id: format!("grant-host-program-{}", nucleus::new_uid("req")),
                program: ProgramAst {
                    schema: ProgramSchema::V1,
                    slug: slug("grant.host"),
                    purpose: "Host Program for grant Actions".to_string(),
                    tags: BTreeSet::new(),
                    parameters: BTreeMap::new(),
                    nodes: BTreeMap::new(),
                    outputs: BTreeMap::new(),
                    required_capabilities: CapabilitySet::default(),
                },
                owner_person_uid: Some(PERSON_UID.to_string()),
            },
            None,
            now(-1),
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

fn request(program_uid: &str) -> GrantAuthorityRequest {
    GrantAuthorityRequest {
        principal_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        program_uid: uid(ReferenceKind::Program, program_uid),
        program_revision_hash: nucleus::karma::CanonicalHash::parse(format!(
            "sha256:{}",
            "a".repeat(64)
        ))
        .unwrap(),
        candidate_template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: None,
        logical_at: at(500),
    }
}

fn spec(program_uid: &str, capabilities: CapabilitySet) -> DelegationGrantSpec {
    DelegationGrantSpec {
        schema: DelegationGrantSchema::V1,
        purpose: "Keep the pantry supplied".to_string(),
        program_uid: uid(ReferenceKind::Program, program_uid),
        program_revision: GrantProgramRevisionScope::AnyActive,
        candidate_templates: GrantTemplateScope::Any,
        capabilities,
        targets: GrantTargetScope::Any,
        budget: Default::default(),
        valid_from: at(0),
        expires_at: at(1_000),
    }
}

fn two_capabilities() -> CapabilitySet {
    CapabilitySet::new([Capability::RecordAddQuantity, Capability::LinkCreate])
}

fn one_capability() -> CapabilitySet {
    CapabilitySet::new([Capability::RecordAddQuantity])
}

fn uid(kind: ReferenceKind, value: &str) -> TypedUid {
    TypedUid::new(kind, value.to_string()).unwrap()
}

fn slug(value: &str) -> Slug {
    Slug::new(value).unwrap()
}

fn at(milliseconds: i64) -> TimestampMs {
    TimestampMs::from_millis(milliseconds).unwrap()
}

fn now(offset_seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(1_784_000_000_000 + offset_seconds * 1_000)
        .single()
        .unwrap()
}
