#![allow(dead_code)]

use engine::Engine;
use engine::actions::{
    Action, TransferPromiseInput, TransferReservePoint, TransferSatiation, TransferVisibility,
};
use engine::trust::Signer;
use nucleus::RecordKind;
use nucleus::transfer::{AgreementType, OpenPromiseReusePolicy, TransferRemainderPolicy};
use protein::{Include, Predicate, Protein, Source};
use std::future::Future;

#[derive(Clone)]
pub struct Person {
    pub uid: String,
    pub signer: Signer,
}

pub struct TransferFixture {
    pub transfer: String,
    pub revision: u64,
}

pub struct DraftOptions<'a> {
    pub slug: &'a str,
    pub agreement: AgreementType,
    pub reserve_default: TransferReservePoint,
    pub require_confirmation: bool,
}

pub fn run_async_test<F, Fut>(name: &str, test: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime")
                .block_on(test());
        })
        .expect("transfer test thread")
        .join()
        .expect("transfer test thread completes");
}

pub async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://transfer.test")
        .await
        .expect("local Organ");
    engine
}

pub async fn person(engine: &Engine, slug: &str) -> Person {
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Person,
                head: slug.into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .expect("Person record")
        .created
        .expect("Person uid");
    Person {
        signer: Signer::generate(&uid, &format!("test:{slug}:transfer")),
        uid,
    }
}

pub async fn plain(engine: &Engine, slug: &str, quantity: f64) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity,
            },
            None,
        )
        .await
        .expect("plain record")
        .created
        .expect("plain record uid")
}

pub fn promise(uid: &str, record: &str, person: &Person, delta: f64) -> TransferPromiseInput {
    TransferPromiseInput {
        uid: Some(uid.into()),
        record: record.into(),
        party: Some(person.uid.clone()),
        open: false,
        delta,
        unit: None,
        window_start: None,
        window_end: None,
        place: None,
        condition: None,
        reserve_from: Some(TransferReservePoint::Inherit),
        reuse_policy: OpenPromiseReusePolicy::Duplicate,
        withdrawn: false,
    }
}

pub async fn create_transfer(
    engine: &Engine,
    creator: &Person,
    invitees: &[Person],
    promises: Vec<TransferPromiseInput>,
    options: DraftOptions<'_>,
) -> TransferFixture {
    engine
        .set_signer(creator.signer.clone())
        .await
        .expect("creator signer");
    let transfer = engine
        .act(
            Action::CreateTransferDraft {
                request_id: format!("create:{}", options.slug),
                creator: Some(creator.uid.clone()),
                slug: Some(options.slug.into()),
                head: options.slug.into(),
                agreement: options.agreement,
                agreement_pct: None,
                satiation: TransferSatiation::None,
                parent: None,
                source: None,
                visibility: TransferVisibility::Hidden,
                max_proximity: None,
                reserve_default: options.reserve_default,
                require_confirmation: options.require_confirmation,
                default_place: None,
                invitees: invitees.iter().map(|person| person.uid.clone()).collect(),
                promises,
                dependencies: Vec::new(),
            },
            None,
        )
        .await
        .expect("signed Transfer draft")
        .created
        .expect("Transfer uid");

    for invitee in invitees {
        let invitation = store::transfers::invitations_for_transfer(&engine.store.pool, &transfer)
            .await
            .expect("Transfer invitations")
            .into_iter()
            .find(|invitation| invitation.addressed_person_uid == invitee.uid)
            .expect("invitee invitation");
        let revision = current_revision(engine, &transfer).await;
        engine
            .set_signer(invitee.signer.clone())
            .await
            .expect("invitee signer");
        engine
            .act(
                Action::AcceptTransferInvitation {
                    invitation: invitation.uid,
                    expected_revision: revision,
                    request_id: format!("accept:{}:{}", options.slug, invitee.uid),
                    transfer: Some(transfer.clone()),
                    person: Some(invitee.uid.clone()),
                },
                None,
            )
            .await
            .expect("accept Transfer invitation");
    }

    TransferFixture {
        revision: current_revision(engine, &transfer).await,
        transfer,
    }
}

pub async fn current_revision(engine: &Engine, transfer: &str) -> u64 {
    store::transfers::get(&engine.store.pool, transfer)
        .await
        .expect("Transfer lookup")
        .expect("Transfer")
        .revision as u64
}

pub async fn agree(engine: &Engine, fixture: &TransferFixture, person: &Person, prefix: &str) {
    for (level, suffix) in [(1, "checked"), (2, "agreed")] {
        engine
            .set_signer(person.signer.clone())
            .await
            .expect("agreement signer");
        engine
            .act(
                Action::SetTransferAgreementLevel {
                    transfer: fixture.transfer.clone(),
                    expected_revision: fixture.revision,
                    request_id: format!("{prefix}:{suffix}"),
                    person: Some(person.uid.clone()),
                    level,
                },
                None,
            )
            .await
            .expect("adjacent agreement transition");
    }
}

pub async fn activate(
    engine: &Engine,
    fixture: &TransferFixture,
    person: &Person,
    promise: &str,
    request_id: &str,
) -> String {
    engine
        .set_signer(person.signer.clone())
        .await
        .expect("activation signer");
    engine
        .act(
            Action::ActivateTransferOccurrence {
                transfer: fixture.transfer.clone(),
                promise: promise.into(),
                expected_revision: fixture.revision,
                request_id: request_id.into(),
                person: Some(person.uid.clone()),
            },
            None,
        )
        .await
        .expect("activate occurrence")
        .created
        .expect("occurrence uid")
}

pub async fn claim(
    engine: &Engine,
    occurrence: &str,
    person: &Person,
    role: engine::actions::TransferOccurrenceClaimRole,
    request_id: &str,
) {
    engine
        .set_signer(person.signer.clone())
        .await
        .expect("claim signer");
    engine
        .act(
            Action::SetTransferOccurrenceClaim {
                occurrence: occurrence.into(),
                request_id: request_id.into(),
                person: Some(person.uid.clone()),
                role,
                claimed: true,
            },
            None,
        )
        .await
        .expect("occurrence claim");
}

pub async fn settlement_preview(
    engine: &Engine,
    occurrence: &str,
    person: &Person,
    quantity: f64,
) -> serde_json::Value {
    let query = Protein {
        source: Source::TransferSettlementPreview,
        filter: vec![
            Predicate::UidEq(occurrence.into()),
            Predicate::QuantityEq(quantity),
        ],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    };
    protein::execute_for_with_signer(&engine.store, &query, None, Some(&person.uid))
        .await
        .expect("settlement preview")
        .into_iter()
        .next()
        .expect("authorized settlement preview row")
}

pub async fn settle_from_preview(
    engine: &Engine,
    occurrence: &str,
    person: &Person,
    request_id: &str,
    preview: &serde_json::Value,
) -> String {
    let remainder_policy = match preview["expected_remainder_policy"].as_str() {
        Some("local_draft") => TransferRemainderPolicy::LocalDraft,
        _ => TransferRemainderPolicy::Visible,
    };
    engine
        .set_signer(person.signer.clone())
        .await
        .expect("settlement signer");
    engine
        .act(
            Action::SettleTransferOccurrence {
                occurrence: occurrence.into(),
                request_id: request_id.into(),
                person: Some(person.uid.clone()),
                canonical_quantity: preview["canonical_quantity"]
                    .as_f64()
                    .expect("preview canonical quantity"),
                expected_remaining_quantity: preview["expected_remaining_quantity"]
                    .as_f64()
                    .expect("preview remaining quantity"),
                expected_local_delta: preview["expected_local_delta"]
                    .as_f64()
                    .expect("preview local delta"),
                expected_application_formula_hash: preview["expected_application_formula_hash"]
                    .as_str()
                    .expect("preview formula hash")
                    .into(),
                expected_application_formula_version:
                    preview["expected_application_formula_version"]
                        .as_u64()
                        .expect("preview formula version"),
                expected_remainder_policy: remainder_policy,
            },
            None,
        )
        .await
        .expect("settle reviewed occurrence slice")
        .created
        .expect("settlement slice uid")
}

#[allow(dead_code)]
pub async fn declare_rule(
    engine: &Engine,
    target: &str,
    cadence: nucleus::karma::Cadence,
    anchor_at: &str,
    condition: Option<&str>,
    gate: Option<&str>,
    carry: Option<&str>,
    consequences: Vec<nucleus::karma::Consequence>,
) -> String {
    engine
        .act(
            engine::actions::Action::CreateRecurrence {
                target: target.to_string(),
                consequences,
                condition: condition.map(str::to_string),
                gate: gate.map(str::to_string),
                carry: carry.map(str::to_string),
                note: None,
                cadence,
                anchor_at: Some(anchor_at.to_string()),
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await
        .expect("a rule is declared")
        .created
        .expect("a rule uid")
}
