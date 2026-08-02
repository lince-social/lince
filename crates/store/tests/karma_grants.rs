use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use nucleus::karma::{
    CanonicalHash, Capability, CapabilitySet, DelegationGrantSchema, DelegationGrantSpec,
    DelegationSignature, GrantAuthorityDenial, GrantAuthorityRequest, GrantProgramRevisionScope,
    GrantStatus, GrantTarget, GrantTargetScope, GrantTemplateScope, ProgramAst, ProgramSchema,
    ReferenceKind, Slug, TimestampMs, TypedUid,
};
use store::Store;
use store::karma::grants::{
    ActivateGrantInput, CreateGrantInput, GrantHandleRow, GrantMutationCommit, NarrowGrantInput,
    RevokeGrantInput, activate, create, evaluate_active, get_handle, get_revision, narrow, revoke,
};
use store::karma::programs::{CreateProgramInput, ProgramMutationCommit};

const PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const OTHER_PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAW";
const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAX";
const OTHER_RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAY";
const MISSING_GRANT_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAZ";

/// create → activate → narrow → revoke, with authority read back from storage at
/// every step. The grant is inert until activation and dead after revocation.
#[tokio::test]
async fn the_grant_lifecycle_moves_authority_only_at_explicit_steps() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.host", "grant-program-1").await;

    let (created, created_fact) = committed(
        create(
            &store.pool,
            CreateGrantInput {
                request_id: "grant-create-1".to_string(),
                slug: slug("household.restock"),
                grant: spec(&program.record_uid, two_capabilities()),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(created.handle_revision, 1);
    assert_eq!(created.status, GrantStatus::Draft);
    assert_eq!(created.active_revision_hash, None);
    assert_eq!(created.principal_person_uid, PERSON_UID);
    // Creating authority is not holding it: no quantity moves and nothing evaluates.
    assert_eq!(created_fact.delta, store::exact::from_f64(0.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);
    assert!(created_fact.signature.is_some());
    assert_eq!(
        denials(&store, &created.record_uid, &request(&program.record_uid)).await,
        vec![GrantAuthorityDenial::GrantInactive]
    );

    let (active, activate_fact) = committed(
        activate(
            &store.pool,
            ActivateGrantInput {
                request_id: "grant-activate-1".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash.clone(),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(active.handle_revision, 2);
    assert_eq!(active.status, GrantStatus::Active);
    assert_eq!(
        active.active_revision_hash.as_ref(),
        Some(&created.head_revision_hash)
    );
    assert_eq!(activate_fact.delta, store::exact::from_f64(1.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 1.0);
    let evaluation = evaluate_active(
        &store.pool,
        &created.record_uid,
        &request(&program.record_uid),
    )
    .await
    .unwrap();
    assert!(evaluation.decision.allowed);
    assert_eq!(evaluation.handle_revision, Some(2));
    assert_eq!(
        evaluation.revision_hash.as_ref(),
        Some(&created.head_revision_hash)
    );

    // Narrowing an active grant swaps head and active together; no wider revision
    // survives the commit.
    let (narrowed, narrow_fact) = committed(
        narrow(
            &store.pool,
            NarrowGrantInput {
                request_id: "grant-narrow-1".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 2,
                grant: spec(&program.record_uid, one_capability()),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(2),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(narrowed.handle_revision, 3);
    assert_eq!(narrowed.status, GrantStatus::Active);
    assert_ne!(narrowed.head_revision_hash, created.head_revision_hash);
    assert_eq!(
        narrowed.active_revision_hash.as_ref(),
        Some(&narrowed.head_revision_hash)
    );
    assert_eq!(narrow_fact.delta, store::exact::from_f64(0.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 1.0);
    let mut dropped = request(&program.record_uid);
    dropped.capability = Capability::LinkCreate;
    assert_eq!(
        denials(&store, &created.record_uid, &dropped).await,
        vec![GrantAuthorityDenial::CapabilityMissing]
    );
    assert!(
        evaluate_active(
            &store.pool,
            &created.record_uid,
            &request(&program.record_uid)
        )
        .await
        .unwrap()
        .decision
        .allowed
    );

    let (revoked, revoke_fact) = committed(
        revoke(
            &store.pool,
            RevokeGrantInput {
                request_id: "grant-revoke-1".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 3,
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(3),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(revoked.status, GrantStatus::Revoked);
    assert_eq!(revoked.active_revision_hash, None);
    assert_eq!(revoke_fact.delta, store::exact::from_f64(-1.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);
    assert_eq!(
        denials(&store, &created.record_uid, &request(&program.record_uid)).await,
        vec![GrantAuthorityDenial::GrantRevoked]
    );

    // A revoked handle cannot be resurrected by any route.
    assert!(
        activate(
            &store.pool,
            ActivateGrantInput {
                request_id: "grant-activate-2".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 4,
                revision_hash: narrowed.head_revision_hash.clone(),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(4),
            signer,
        )
        .await
        .is_err()
    );
    assert!(
        narrow(
            &store.pool,
            NarrowGrantInput {
                request_id: "grant-narrow-2".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 4,
                grant: spec(&program.record_uid, one_capability()),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(4),
            signer,
        )
        .await
        .is_err()
    );
    // Every revision ever published stays readable; only the live pointer moved.
    assert_eq!(count(&store, "karma_grant_revision").await, 2);
    assert!(
        get_revision(
            &store.pool,
            &created.record_uid,
            &created.head_revision_hash
        )
        .await
        .unwrap()
        .is_some()
    );
}

/// Narrowing a draft moves the head without ever making the grant live. The
/// handle CHECK ties status to the active pointer, so this transition has to
/// leave `active_revision_hash` NULL rather than write it.
#[tokio::test]
async fn narrowing_a_draft_grant_never_activates_it() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.narrow.draft", "grant-program-narrow-draft").await;
    let (created, _) = committed(
        create(
            &store.pool,
            create_input(
                &program.record_uid,
                "narrow.draft",
                "grant-create-narrow-draft",
            ),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );

    let (narrowed, fact) = committed(
        narrow(
            &store.pool,
            NarrowGrantInput {
                request_id: "grant-narrow-draft".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                grant: spec(&program.record_uid, one_capability()),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(narrowed.handle_revision, 2);
    assert_eq!(narrowed.status, GrantStatus::Draft);
    assert_eq!(narrowed.active_revision_hash, None);
    assert_ne!(narrowed.head_revision_hash, created.head_revision_hash);
    assert_eq!(fact.delta, store::exact::from_f64(0.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);
    assert_eq!(
        denials(&store, &created.record_uid, &request(&program.record_uid)).await,
        vec![GrantAuthorityDenial::GrantInactive]
    );
}

/// Revoking a draft grant that never held authority must not push the Record
/// quantity negative: the delta follows the transition, not the action name.
#[tokio::test]
async fn revoking_a_draft_grant_moves_no_authority_quantity() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.draft", "grant-program-draft").await;
    let (created, _) = committed(
        create(
            &store.pool,
            create_input(&program.record_uid, "draft.only", "grant-create-draft"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );

    let (revoked, fact) = committed(
        revoke(
            &store.pool,
            RevokeGrantInput {
                request_id: "grant-revoke-draft".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(revoked.status, GrantStatus::Revoked);
    assert_eq!(fact.delta, store::exact::from_f64(0.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);
}

#[tokio::test]
async fn replay_is_exact_and_stale_expectations_write_nothing() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.replay", "grant-program-2").await;
    let input = create_input(&program.record_uid, "replay.one", "grant-create-replay");

    let (created, created_fact) = committed(
        create(&store.pool, input.clone(), now(0), signer)
            .await
            .unwrap(),
    );
    let facts_after_create = count(&store, "fact").await;

    let (replayed_handle, replayed_fact) = replayed(
        create(&store.pool, input.clone(), now(9), signer)
            .await
            .unwrap(),
    );
    assert_eq!(replayed_handle, created);
    assert_eq!(replayed_fact, created_fact);
    assert_eq!(count(&store, "fact").await, facts_after_create);
    assert_eq!(count(&store, "karma_grant").await, 1);
    assert_eq!(count(&store, "karma_grant_revision").await, 1);
    assert_eq!(count(&store, "karma_grant_request").await, 1);

    // Same request id, different payload: refused rather than silently accepted.
    let mut altered = input.clone();
    altered.slug = slug("replay.two");
    assert!(create(&store.pool, altered, now(10), signer).await.is_err());

    let stale = activate(
        &store.pool,
        ActivateGrantInput {
            request_id: "grant-activate-stale".to_string(),
            grant_uid: created.record_uid.clone(),
            expected_handle_revision: 7,
            revision_hash: created.head_revision_hash.clone(),
            actor_person_uid: PERSON_UID.to_string(),
        },
        now(11),
        signer,
    )
    .await
    .unwrap();
    assert_eq!(
        stale,
        GrantMutationCommit::Stale {
            current_handle_revision: 1
        }
    );
    assert_eq!(count(&store, "fact").await, facts_after_create);
    assert_eq!(count(&store, "karma_grant_request").await, 1);
    assert_eq!(
        get_handle(&store.pool, &created.record_uid)
            .await
            .unwrap()
            .unwrap(),
        created
    );
}

#[tokio::test]
async fn stored_revisions_and_requests_are_immutable() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.frozen", "grant-program-3").await;
    let (created, _) = committed(
        create(
            &store.pool,
            create_input(&program.record_uid, "frozen.one", "grant-create-frozen"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );

    for statement in [
        "UPDATE karma_grant_revision SET revision_json = '{}'",
        "DELETE FROM karma_grant_revision",
        "UPDATE karma_grant_request SET result_handle_revision = 9",
        "DELETE FROM karma_grant_request",
    ] {
        assert!(
            sqlx::query(statement).execute(&store.pool).await.is_err(),
            "{statement} must be refused"
        );
    }
    assert_eq!(
        get_handle(&store.pool, &created.record_uid)
            .await
            .unwrap()
            .unwrap(),
        created
    );
}

#[tokio::test]
async fn only_a_proven_subset_of_a_live_program_can_be_granted() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.subset", "grant-program-4").await;
    let other = host_program(&store, "grant.other", "grant-program-5").await;

    // The Program must exist.
    let mut unknown = create_input(
        &program.record_uid,
        "unknown.program",
        "grant-create-unknown",
    );
    unknown.grant.program_uid = uid(ReferenceKind::Program, MISSING_GRANT_UID);
    assert!(create(&store.pool, unknown, now(0), signer).await.is_err());

    // An exact revision scope must belong to its own Program.
    let mut foreign = create_input(&program.record_uid, "foreign.rev", "grant-create-foreign");
    foreign.grant.program_revision = GrantProgramRevisionScope::Exact {
        revision_hash: other.head_revision_hash.clone(),
    };
    assert!(create(&store.pool, foreign, now(1), signer).await.is_err());

    // Grant management capability is never delegable through a grant.
    let mut escalating = create_input(&program.record_uid, "escalate", "grant-create-escalate");
    escalating.grant.capabilities = CapabilitySet::new([Capability::KarmaGrantWiden]);
    assert!(
        create(&store.pool, escalating, now(2), signer)
            .await
            .is_err()
    );

    let (created, _) = committed(
        create(
            &store.pool,
            create_input(&program.record_uid, "subset.one", "grant-create-subset"),
            now(3),
            signer,
        )
        .await
        .unwrap(),
    );

    // Widening is refused even though the actor is the principal.
    let mut wider = spec(&program.record_uid, two_capabilities());
    wider.capabilities = CapabilitySet::new([
        Capability::RecordAddQuantity,
        Capability::LinkCreate,
        Capability::MetadataWrite,
    ]);
    assert!(
        narrow(
            &store.pool,
            NarrowGrantInput {
                request_id: "grant-narrow-wider".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                grant: wider,
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(4),
            signer,
        )
        .await
        .is_err()
    );

    // An unchanged replacement is not a narrowing either.
    assert!(
        narrow(
            &store.pool,
            NarrowGrantInput {
                request_id: "grant-narrow-equal".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                grant: spec(&program.record_uid, two_capabilities()),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(5),
            signer,
        )
        .await
        .is_err()
    );
    assert_eq!(count(&store, "karma_grant_revision").await, 1);
}

#[tokio::test]
async fn only_the_signing_principal_may_hold_or_change_a_grant() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.principal", "grant-program-6").await;

    // No signature, no grant.
    assert!(
        create(
            &store.pool,
            create_input(&program.record_uid, "unsigned", "grant-create-unsigned"),
            now(0),
            |_: &str| None,
        )
        .await
        .is_err()
    );
    // A signature from anyone but the principal is refused.
    assert!(
        create(
            &store.pool,
            create_input(&program.record_uid, "misigned", "grant-create-misigned"),
            now(1),
            |hash: &str| Some(DelegationSignature {
                signer_person_uid: uid(ReferenceKind::Person, OTHER_PERSON_UID),
                key_id: "key-other".to_string(),
                signature: format!("signed:{hash}"),
            }),
        )
        .await
        .is_err()
    );
    assert_eq!(count(&store, "karma_grant").await, 0);

    let (created, _) = committed(
        create(
            &store.pool,
            create_input(
                &program.record_uid,
                "principal.one",
                "grant-create-principal",
            ),
            now(2),
            signer,
        )
        .await
        .unwrap(),
    );
    // A different Person cannot activate, narrow, or revoke someone else's grant.
    assert!(
        activate(
            &store.pool,
            ActivateGrantInput {
                request_id: "grant-activate-other".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash.clone(),
                actor_person_uid: OTHER_PERSON_UID.to_string(),
            },
            now(3),
            other_signer,
        )
        .await
        .is_err()
    );
    assert!(
        revoke(
            &store.pool,
            RevokeGrantInput {
                request_id: "grant-revoke-other".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                actor_person_uid: OTHER_PERSON_UID.to_string(),
            },
            now(4),
            other_signer,
        )
        .await
        .is_err()
    );
    assert_eq!(
        get_handle(&store.pool, &created.record_uid)
            .await
            .unwrap()
            .unwrap()
            .status,
        GrantStatus::Draft
    );
}

/// Every dimension of the frozen request denies on its own, and an absent grant
/// denies without consulting anything else.
#[tokio::test]
async fn authority_denies_on_each_dimension_separately() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.deny", "grant-program-7").await;
    let other = host_program(&store, "grant.deny.other", "grant-program-8").await;

    let mut scoped = spec(&program.record_uid, two_capabilities());
    scoped.program_revision = GrantProgramRevisionScope::Exact {
        revision_hash: program.head_revision_hash.clone(),
    };
    scoped.candidate_templates = GrantTemplateScope::Only {
        templates: BTreeSet::from([slug("record.add-quantity")]),
    };
    scoped.targets = GrantTargetScope::Only {
        targets: BTreeSet::from([GrantTarget::Record(uid(ReferenceKind::Record, RECORD_UID))]),
    };
    let (created, _) = committed(
        create(
            &store.pool,
            CreateGrantInput {
                request_id: "grant-create-deny".to_string(),
                slug: slug("deny.one"),
                grant: scoped,
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    committed(
        activate(
            &store.pool,
            ActivateGrantInput {
                request_id: "grant-activate-deny".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash.clone(),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );

    let allowed = GrantAuthorityRequest {
        principal_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        program_uid: uid(ReferenceKind::Program, &program.record_uid),
        program_revision_hash: program.head_revision_hash.clone(),
        candidate_template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: Some(GrantTarget::Record(uid(ReferenceKind::Record, RECORD_UID))),
        logical_at: at(500),
    };
    assert!(
        evaluate_active(&store.pool, &created.record_uid, &allowed)
            .await
            .unwrap()
            .decision
            .allowed
    );

    let mismatches: Vec<(GrantAuthorityRequest, GrantAuthorityDenial)> = vec![
        (
            with(&allowed, |r| {
                r.principal_person_uid = uid(ReferenceKind::Person, OTHER_PERSON_UID)
            }),
            GrantAuthorityDenial::PrincipalMismatch,
        ),
        (
            with(&allowed, |r| {
                r.program_uid = uid(ReferenceKind::Program, &other.record_uid)
            }),
            GrantAuthorityDenial::ProgramMismatch,
        ),
        (
            with(&allowed, |r| {
                r.program_revision_hash = other.head_revision_hash.clone()
            }),
            GrantAuthorityDenial::ProgramRevisionMismatch,
        ),
        (
            with(&allowed, |r| {
                r.candidate_template = slug("record.set-quantity")
            }),
            GrantAuthorityDenial::CandidateTemplateMismatch,
        ),
        (
            with(&allowed, |r| r.capability = Capability::MetadataWrite),
            GrantAuthorityDenial::CapabilityMissing,
        ),
        (
            with(&allowed, |r| r.target = None),
            GrantAuthorityDenial::TargetRequired,
        ),
        (
            with(&allowed, |r| {
                r.target = Some(GrantTarget::Record(uid(
                    ReferenceKind::Record,
                    OTHER_RECORD_UID,
                )))
            }),
            GrantAuthorityDenial::TargetMismatch,
        ),
        (
            with(&allowed, |r| r.logical_at = at(-1)),
            GrantAuthorityDenial::NotYetValid,
        ),
        (
            with(&allowed, |r| r.logical_at = at(10_000)),
            GrantAuthorityDenial::Expired,
        ),
    ];
    for (request, expected) in mismatches {
        let decision = evaluate_active(&store.pool, &created.record_uid, &request)
            .await
            .unwrap()
            .decision;
        assert!(!decision.allowed, "{expected:?} must deny");
        assert_eq!(decision.denials, vec![expected]);
    }

    let missing = evaluate_active(&store.pool, MISSING_GRANT_UID, &allowed)
        .await
        .unwrap();
    assert_eq!(
        missing.decision.denials,
        vec![GrantAuthorityDenial::GrantMissing]
    );
    assert_eq!(missing.handle_revision, None);
}

/// K5.1 is an authority boundary and nothing more: no intent, receipt, or
/// candidate row may appear anywhere along the lifecycle.
#[tokio::test]
async fn granting_authority_creates_no_effect_of_any_kind() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.inert", "grant-program-9").await;
    let (created, _) = committed(
        create(
            &store.pool,
            create_input(&program.record_uid, "inert.one", "grant-create-inert"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    committed(
        activate(
            &store.pool,
            ActivateGrantInput {
                request_id: "grant-activate-inert".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash.clone(),
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );

    for table in [
        "karma_candidate",
        "karma_candidate_review_event",
        "karma_occurrence",
        "karma_run",
        "transfer",
    ] {
        assert_eq!(count(&store, table).await, 0, "{table} must stay empty");
    }
}

fn with(
    request: &GrantAuthorityRequest,
    edit: impl FnOnce(&mut GrantAuthorityRequest),
) -> GrantAuthorityRequest {
    let mut request = request.clone();
    edit(&mut request);
    request
}

async fn denials(
    store: &Store,
    grant_uid: &str,
    request: &GrantAuthorityRequest,
) -> Vec<GrantAuthorityDenial> {
    evaluate_active(&store.pool, grant_uid, request)
        .await
        .unwrap()
        .decision
        .denials
}

fn request(program_uid: &str) -> GrantAuthorityRequest {
    GrantAuthorityRequest {
        principal_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        program_uid: uid(ReferenceKind::Program, program_uid),
        program_revision_hash: hash('a'),
        candidate_template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: None,
        logical_at: at(500),
    }
}

fn create_input(program_uid: &str, slug_text: &str, request_id: &str) -> CreateGrantInput {
    CreateGrantInput {
        request_id: request_id.to_string(),
        slug: slug(slug_text),
        grant: spec(program_uid, two_capabilities()),
        actor_person_uid: PERSON_UID.to_string(),
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

async fn host_program(store: &Store, slug_text: &str, request_id: &str) -> ProgramSummary {
    let ast = ProgramAst {
        schema: ProgramSchema::V1,
        slug: slug(slug_text),
        purpose: "Host Program for grant tests".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    };
    let commit = store::karma::programs::create(
        &store.pool,
        CreateProgramInput {
            request_id: request_id.to_string(),
            program: ast,
            owner_person_uid: Some(PERSON_UID.to_string()),
            actor_person_uid: Some(PERSON_UID.to_string()),
        },
        now(-1),
        |hash: &str| Some(format!("signed:{hash}")),
    )
    .await
    .unwrap();
    match commit {
        ProgramMutationCommit::Committed { handle, .. } => ProgramSummary {
            record_uid: handle.record_uid,
            head_revision_hash: handle.head_revision_hash,
        },
        other => panic!("expected a created Program, got {other:?}"),
    }
}

struct ProgramSummary {
    record_uid: String,
    head_revision_hash: CanonicalHash,
}

fn committed(commit: GrantMutationCommit) -> (GrantHandleRow, nucleus::Fact) {
    match commit {
        GrantMutationCommit::Committed { handle, fact } => (handle, fact),
        other => panic!("expected a committed grant mutation, got {other:?}"),
    }
}

fn replayed(commit: GrantMutationCommit) -> (GrantHandleRow, nucleus::Fact) {
    match commit {
        GrantMutationCommit::Replayed { handle, fact } => (handle, fact),
        other => panic!("expected a replayed grant mutation, got {other:?}"),
    }
}

fn signer(hash: &str) -> Option<DelegationSignature> {
    Some(DelegationSignature {
        signer_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        key_id: "key-ana".to_string(),
        signature: format!("signed:{hash}"),
    })
}

fn other_signer(hash: &str) -> Option<DelegationSignature> {
    Some(DelegationSignature {
        signer_person_uid: uid(ReferenceKind::Person, OTHER_PERSON_UID),
        key_id: "key-other".to_string(),
        signature: format!("signed:{hash}"),
    })
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

fn hash(fill: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", String::from_iter([fill; 64]))).unwrap()
}

fn now(offset_seconds: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-24T12:00:00.000Z")
        .unwrap()
        .with_timezone(&Utc)
        + chrono::TimeDelta::seconds(offset_seconds)
}

async fn count(store: &Store, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn record_quantity(store: &Store, uid: &str) -> f64 {
    store::records::get(&store.pool, uid)
        .await
        .unwrap()
        .unwrap()
        .quantity_f64()
}

/// The contract says a revoked handle is replaced by creating a new grant, so
/// two grants must be able to carry identical terms.
#[tokio::test]
async fn a_revoked_grant_can_be_replaced_by_an_identical_one() {
    let store = Store::open_memory().await.unwrap();
    let program = host_program(&store, "grant.replace", "grant-program-replace").await;
    let (created, _) = committed(
        create(
            &store.pool,
            create_input(&program.record_uid, "replace.one", "grant-create-replace"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    committed(
        revoke(
            &store.pool,
            RevokeGrantInput {
                request_id: "grant-revoke-replace".to_string(),
                grant_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                actor_person_uid: PERSON_UID.to_string(),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );

    let (replacement, _) = committed(
        create(
            &store.pool,
            create_input(
                &program.record_uid,
                "replace.two",
                "grant-create-replacement",
            ),
            now(2),
            signer,
        )
        .await
        .expect("renewed consent must be expressible"),
    );
    assert_ne!(replacement.record_uid, created.record_uid);
    assert_eq!(replacement.status, GrantStatus::Draft);
}
