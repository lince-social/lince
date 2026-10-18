use crate::StoreError;
use nucleus::transfer::transport::{Event, Journey, PICKUP_GRACE_MILLIS, Progress, Step};
use nucleus::transfer::{
    disclosure::TransferItem,
    transport::{Capacity, Direction, Terms},
};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::BTreeMap;

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

pub async fn journey(pool: &SqlitePool, transfer: &str) -> Result<Option<Journey>, StoreError> {
    let mut connection = pool.acquire().await?;
    journey_on(&mut connection, transfer).await
}

pub async fn journey_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<Journey>, StoreError> {
    let row:Option<(i64,Option<String>)>=sqlx::query_as("SELECT t.revision, MIN(p.window_end) FROM transfer t JOIN record r ON r.uid=t.record_uid AND r.deleted_at IS NULL JOIN promise p ON p.transfer_uid=t.record_uid AND p.state!='withdrawn' WHERE t.record_uid=? AND json_extract(p.item_json,'$.transport.activity') IS NOT NULL GROUP BY t.record_uid")
        .bind(transfer).fetch_optional(&mut *connection).await?;
    let Some((revision, until)) = row else {
        return Ok(None);
    };
    let until = chrono::DateTime::parse_from_rfc3339(
        &until.ok_or_else(|| invalid("Transport pickup window unavailable"))?,
    )
    .map_err(invalid)?
    .timestamp_millis()
    .checked_add(PICKUP_GRACE_MILLIS)
    .ok_or_else(|| invalid("Pickup grace interval overflows"))?;
    let people:Vec<(String,i64)>=sqlx::query_as("SELECT p.actor_uid,COALESCE(a.level,0) FROM transfer_party p LEFT JOIN transfer_agreement a ON a.party_uid=p.uid AND a.transfer_uid=p.transfer_uid AND a.revision=? WHERE p.transfer_uid=? ORDER BY p.actor_uid").bind(revision).bind(transfer).fetch_all(&mut *connection).await?;
    let agreed = !people.is_empty() && people.iter().all(|(_, level)| *level == 2);
    let mut participants = people
        .into_iter()
        .map(|(person, _)| {
            (
                person.clone(),
                Progress {
                    person,
                    step: None,
                    fact: None,
                    at: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let rows=sqlx::query("SELECT uid,actor_uid,at,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END AS payload FROM fact WHERE record_uid=? AND delta_mantissa='0' AND cause_kind='user_edit' AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.journey.domain')=? AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.journey.revision')=? ORDER BY rowid LIMIT 1025")
        .bind(transfer).bind(nucleus::transfer::transport::JOURNEY_DOMAIN).bind(revision).fetch_all(&mut *connection).await?;
    if rows.len() > 1024 {
        return Err(invalid("Journey history exceeds its bounded event count"));
    }
    let mut started = false;
    for row in rows {
        let Some(event) = row
            .get::<Option<String>, _>("payload")
            .as_deref()
            .and_then(Event::parse)
        else {
            return Err(invalid("Journey event is unavailable or invalid"));
        };
        if event.transfer != transfer
            || event.revision != revision as u64
            || row.get::<Option<String>, _>("actor_uid").as_deref() != Some(event.person.as_str())
        {
            return Err(invalid("Journey attribution differs from its signed event"));
        }
        let Some(progress) = participants.get_mut(&event.person) else {
            continue;
        };
        started |= matches!(event.step, Step::Departed | Step::Destination);
        if progress.step.is_none_or(|step| event.step > step) {
            progress.step = Some(event.step);
            progress.fact = Some(row.get("uid"));
            progress.at = Some(row.get("at"));
        }
    }
    let expired = !started && nucleus::execution::now().timestamp_millis() >= until;
    let all_finished = !participants.is_empty()
        && participants
            .values()
            .all(|progress| progress.step.is_some_and(Step::terminal));
    let capacity_released = expired
        || participants
            .values()
            .any(|progress| progress.step.is_some_and(Step::terminal));
    Ok(Some(Journey {
        transfer: transfer.into(),
        revision: revision as u64,
        agreed,
        participants: participants.into_values().collect(),
        started,
        expired,
        capacity_released,
        all_finished,
        pickup_grace_until: chrono::DateTime::from_timestamp_millis(until)
            .ok_or_else(|| invalid("Pickup grace timestamp unavailable"))?
            .to_rfc3339(),
    }))
}

async fn terms_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<(Terms, Option<String>)>, StoreError> {
    let row:Option<(String,Option<String>)>=sqlx::query_as("SELECT item_json,party_uid FROM promise WHERE transfer_uid=? AND json_extract(item_json,'$.transport.activity') IS NOT NULL ORDER BY state='withdrawn',delta>0,uid LIMIT 1")
        .bind(transfer).fetch_optional(&mut *connection).await?;
    row.map(|(raw, party)| {
        let item: TransferItem = serde_json::from_str(&raw).map_err(invalid)?;
        let terms = item
            .transport
            .ok_or_else(|| invalid("Transport terms unavailable"))?;
        terms.validate().map_err(invalid)?;
        Ok((terms, item.exchange.map(|route| route.giver).or(party)))
    })
    .transpose()
}

async fn group_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<(String, Terms, Option<String>)>, StoreError> {
    let Some((mut terms, mut giver)) = terms_on(connection, transfer).await? else {
        return Ok(None);
    };
    let mut current = transfer.to_owned();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..8 {
        if !seen.insert(current.clone()) {
            return Err(invalid("Transport capacity references contain a cycle"));
        }
        let references: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT source_uid,parent_uid FROM transfer WHERE record_uid=?")
                .bind(&current)
                .fetch_optional(&mut *connection)
                .await?;
        let mut next = None;
        if let Some((source, parent)) = references {
            for candidate in source.into_iter().chain(parent) {
                if let Some((candidate_terms, candidate_giver)) =
                    terms_on(connection, &candidate).await?
                    && candidate_terms.direction == Direction::Offer
                    && candidate_terms.activity == terms.activity
                {
                    if giver != candidate_giver {
                        return Err(invalid("A booking must use the offering driver's capacity"));
                    }
                    next = Some((candidate, candidate_terms, candidate_giver));
                    break;
                }
            }
        }
        let Some((uid, parent_terms, parent_giver)) = next else {
            return Ok(Some((current, terms, giver)));
        };
        current = uid;
        terms = parent_terms;
        giver = parent_giver;
    }
    Err(invalid("Transport capacity references exceed eight levels"))
}

async fn capacities_on(
    connection: &mut SqliteConnection,
) -> Result<BTreeMap<String, Capacity>, StoreError> {
    let rows=sqlx::query("SELECT p.uid,p.transfer_uid,p.delta,p.item_json FROM promise p JOIN transfer t ON t.record_uid=p.transfer_uid JOIN record r ON r.uid=t.record_uid AND r.deleted_at IS NULL WHERE p.state IN ('agreed','active') AND json_extract(p.item_json,'$.transport.activity') IS NOT NULL ORDER BY p.transfer_uid,p.uid")
        .fetch_all(&mut *connection).await?;
    let mut capacities = BTreeMap::<String, Capacity>::new();
    let mut exchanges = std::collections::BTreeSet::new();
    for row in rows {
        let transfer: String = row.get("transfer_uid");
        if journey_on(connection, &transfer)
            .await?
            .is_some_and(|journey| journey.capacity_released)
        {
            continue;
        }
        let item: TransferItem =
            serde_json::from_str(&row.get::<String, _>("item_json")).map_err(invalid)?;
        let exchange = item
            .exchange
            .as_ref()
            .map(|route| route.uid.clone())
            .unwrap_or_else(|| row.get("uid"));
        if !exchanges.insert((transfer.clone(), exchange)) {
            continue;
        }
        let booking = item
            .transport
            .ok_or_else(|| invalid("Booking terms unavailable"))?;
        let (offer, terms, _) = group_on(connection, &transfer)
            .await?
            .ok_or_else(|| invalid("Capacity offer unavailable"))?;
        if row.get::<f64, _>("delta").abs() != f64::from(booking.count) {
            return Err(invalid("Booking quantity differs from its reviewed count"));
        }
        let capacity = capacities.entry(offer.clone()).or_insert(Capacity {
            offer,
            total: terms.capacity,
            reserved: 0,
            available: terms.capacity,
        });
        capacity.reserved = capacity
            .reserved
            .checked_add(booking.count)
            .ok_or_else(|| invalid("Transport reservation count overflows"))?;
        capacity.available = capacity
            .total
            .checked_sub(capacity.reserved)
            .ok_or_else(|| invalid("Transport capacity would be exceeded"))?;
    }
    Ok(capacities)
}

pub async fn validate_all_on(connection: &mut SqliteConnection) -> Result<(), StoreError> {
    capacities_on(connection).await.map(|_| ())
}

pub async fn capacity(pool: &SqlitePool, transfer: &str) -> Result<Option<Capacity>, StoreError> {
    let mut connection = pool.acquire().await?;
    let Some((offer, terms, _)) = group_on(&mut connection, transfer).await? else {
        return Ok(None);
    };
    let mut capacities = capacities_on(&mut connection).await?;
    Ok(Some(capacities.remove(&offer).unwrap_or(Capacity {
        offer,
        total: terms.capacity,
        reserved: 0,
        available: terms.capacity,
    })))
}
