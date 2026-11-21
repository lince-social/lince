use nucleus::{
    PromiseState,
    transfer::{TransferOccurrenceSnapshot, TransferRevisionEvidence},
};
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

use super::ActivateOccurrencesInput;
use crate::{
    StoreError,
    transfers::{TransferLocationSnapshot, decode_item, location_snapshot},
};

struct ActivationPromiseTerms {
    uid: String,
    exchange: Option<nucleus::transfer::exchange::ExchangeRoute>,
    record_uid: Option<String>,
    concept_uid: Option<String>,
    unit_uid: Option<String>,
    person_uid: String,
    delta: f64,
    window_start: Option<String>,
    window_end: Option<String>,
    location: Option<TransferLocationSnapshot>,
    state: String,
}

fn map_activation_promise(row: SqliteRow) -> Result<ActivationPromiseTerms, StoreError> {
    Ok(ActivationPromiseTerms {
        uid: row.get("uid"),
        exchange: decode_item(row.get("item_json"))?.and_then(|item| item.exchange),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        person_uid: row.get("party_uid"),
        delta: row.get("delta"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        state: row.get("state"),
    })
}

async fn activation_promise(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    promise_uid: &str,
) -> Result<ActivationPromiseTerms, StoreError> {
    let row = sqlx::query(
        "SELECT uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon,
                location_address, state, item_json
         FROM promise WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(promise_uid)
    .bind(transfer_uid)
    .fetch_one(&mut **tx)
    .await?;
    map_activation_promise(row)
}

fn promises_are_exact_opposites(
    promise: &ActivationPromiseTerms,
    opposite: &ActivationPromiseTerms,
) -> bool {
    let same_subject = match (&promise.exchange, &opposite.exchange) {
        (Some(left), Some(right)) => left == right,
        (None, None) => match (&promise.concept_uid, &opposite.concept_uid) {
            (Some(left), Some(right)) => left == right,
            _ => promise.record_uid.is_some() && promise.record_uid == opposite.record_uid,
        },
        _ => false,
    };
    promise.uid != opposite.uid
        && same_subject
        && promise.unit_uid == opposite.unit_uid
        && (promise.delta + opposite.delta).abs() < 1e-9
        && promise.window_start == opposite.window_start
        && promise.window_end == opposite.window_end
        && promise.location == opposite.location
}

pub(super) async fn prepare(
    tx: &mut Transaction<'_, Sqlite>,
    input: &ActivateOccurrencesInput,
    expected_revision: i64,
    agreement_type: &str,
    revision_evidence: &TransferRevisionEvidence,
    at: &str,
) -> Result<Vec<TransferOccurrenceSnapshot>, StoreError> {
    let requires_agreement = agreement_type != "dependency";
    let signed = |uid: &str| {
        revision_evidence
            .terms
            .promises
            .iter()
            .any(|promise| promise.uid == uid && promise.state != "withdrawn")
    };
    let party_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM transfer_party WHERE transfer_uid = ?")
            .bind(&input.transfer_uid)
            .fetch_one(&mut **tx)
            .await?;
    let organ_of = |person: &str| {
        revision_evidence
            .terms
            .parties
            .iter()
            .find(|party| party.person_uid == person)
            .and_then(|party| party.organ_uid.clone())
    };
    let mut snapshots = Vec::with_capacity(input.occurrences.len());
    for requested in &input.occurrences {
        let promise = activation_promise(tx, &input.transfer_uid, &requested.promise_uid).await?;
        if !signed(&promise.uid) {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{}` is not present in the signed current revision",
                promise.uid
            )));
        }
        if !(promise.state == PromiseState::Agreed.as_str()
            || !requires_agreement && promise.state == PromiseState::Proposed.as_str())
            || promise.person_uid != input.actor_person_uid
        {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{}` is not an agreed promise owned by the acting Person",
                promise.uid
            )));
        }
        if (promise.delta < 0.0 && requested.giver_person_uid != input.actor_person_uid)
            || (promise.delta > 0.0 && requested.receiver_person_uid != input.actor_person_uid)
            || !promise.delta.is_finite()
            || promise.delta == 0.0
        {
            return Err(sqlx::Error::Protocol(
                "occurrence direction does not match the acting Person's promise delta".into(),
            ));
        }
        let (roles_exist, agreed_roles): (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(ta.party_uid)
             FROM transfer_party tp
             JOIN record person ON person.uid = tp.actor_uid AND person.kind = 'person'
             LEFT JOIN transfer_agreement ta ON ta.transfer_uid = tp.transfer_uid
                AND ta.party_uid = tp.uid AND ta.revision = ? AND ta.level = 2
             WHERE tp.transfer_uid = ? AND tp.actor_uid IN (?, ?)",
        )
        .bind(expected_revision)
        .bind(&input.transfer_uid)
        .bind(&requested.giver_person_uid)
        .bind(&requested.receiver_person_uid)
        .fetch_one(&mut **tx)
        .await?;
        if requested.giver_person_uid == requested.receiver_person_uid {
            return Err(sqlx::Error::Protocol(
                "occurrence giver and receiver must be different People".into(),
            ));
        }
        if roles_exist != 2 {
            return Err(sqlx::Error::Protocol(
                "occurrence giver and receiver must be accepted Transfer parties".into(),
            ));
        }
        if requires_agreement && agreed_roles != 2 {
            return Err(sqlx::Error::Protocol(
                "occurrence path requires current agreement from giver and receiver".into(),
            ));
        }

        if let Some(exchange) = &promise.exchange
            && (exchange.giver != requested.giver_person_uid
                || exchange.receiver != requested.receiver_person_uid)
        {
            return Err(sqlx::Error::Protocol(
                "activation must use the agreed exchange route".into(),
            ));
        }
        let opposite = if let Some(uid) = requested.opposite_promise_uid.as_deref() {
            let opposite = activation_promise(tx, &input.transfer_uid, uid).await?;
            if !signed(&opposite.uid) {
                return Err(sqlx::Error::Protocol(format!(
                    "opposite promise `{}` is not present in the signed current revision",
                    opposite.uid
                )));
            }
            if !promises_are_exact_opposites(&promise, &opposite)
                || !(matches!(
                    PromiseState::parse(&opposite.state),
                    Some(PromiseState::Agreed | PromiseState::Active)
                ) || !requires_agreement && opposite.state == "proposed")
                || (promise.delta < 0.0 && opposite.person_uid != requested.receiver_person_uid)
                || (promise.delta > 0.0 && opposite.person_uid != requested.giver_person_uid)
            {
                return Err(sqlx::Error::Protocol(
                    "named opposite promise is not an exact opposite path".into(),
                ));
            }
            Some(opposite)
        } else {
            if party_count > 2 && promise.exchange.is_none() {
                return Err(sqlx::Error::Protocol(
                    "unmatched promise is ambiguous in a multi-party Transfer".into(),
                ));
            }
            None
        };
        let (primary_uid, opposite_uid) = if let Some(opposite) = opposite.as_ref() {
            if promise.uid < opposite.uid {
                (promise.uid.as_str(), Some(opposite.uid.as_str()))
            } else {
                (opposite.uid.as_str(), Some(promise.uid.as_str()))
            }
        } else {
            (promise.uid.as_str(), None)
        };
        let exchange_path_uid: String = if let Some(uid) = sqlx::query_scalar(
            "SELECT uid FROM transfer_exchange_path
             WHERE transfer_uid = ? AND revision = ? AND primary_promise_uid = ?
               AND opposite_promise_uid IS ?",
        )
        .bind(&input.transfer_uid)
        .bind(expected_revision)
        .bind(primary_uid)
        .bind(opposite_uid)
        .fetch_optional(&mut **tx)
        .await?
        {
            uid
        } else {
            let uid = nucleus::new_uid("tep");
            sqlx::query(
                "INSERT INTO transfer_exchange_path
                    (uid, transfer_uid, revision, primary_promise_uid,
                     opposite_promise_uid, created_at, public_exchange_uid)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(&input.transfer_uid)
            .bind(expected_revision)
            .bind(primary_uid)
            .bind(opposite_uid)
            .bind(at)
            .bind(
                promise
                    .exchange
                    .as_ref()
                    .map(|exchange| exchange.uid.as_str()),
            )
            .execute(&mut **tx)
            .await?;
            uid
        };
        snapshots.push(TransferOccurrenceSnapshot {
            uid: nucleus::new_uid("toc"),
            promise_uid: promise.uid,
            exchange_path_uid,
            exchange_uid: promise.exchange.map(|exchange| exchange.uid),
            opposite_promise_uid: opposite.map(|value| value.uid),
            record_uid: promise.record_uid,
            concept_uid: promise.concept_uid,
            unit_uid: promise.unit_uid,
            quantity: promise.delta.abs(),
            giver_person_uid: requested.giver_person_uid.clone(),
            receiver_person_uid: requested.receiver_person_uid.clone(),
            giver_organ_uid: organ_of(&requested.giver_person_uid),
            receiver_organ_uid: organ_of(&requested.receiver_person_uid),
            window_start: promise.window_start,
            window_end: promise.window_end,
            location: promise.location,
        });
    }
    snapshots.sort_by(|left, right| left.uid.cmp(&right.uid));
    Ok(snapshots)
}
