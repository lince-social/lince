//! Resolve offering routes and reserve each booking on its ordered segments.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use nucleus::transfer::{
    disclosure::TransferItem,
    transport::{Capacity, Direction, SegmentCapacity, Terms},
};
use serde_json::from_str;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{invalid, journey_on};
use crate::StoreError;

async fn terms_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<(Terms, Option<String>)>, StoreError> {
    sqlx::query(
        "SELECT item_json, party_uid FROM promise
         WHERE transfer_uid = ? AND json_extract(item_json, '$.transport.activity') IS NOT NULL
         ORDER BY state = 'withdrawn', delta > 0, uid LIMIT 1",
    )
    .bind(transfer)
    .try_map(|row| {
        let source: &str = row.try_get("item_json")?;
        let party: Option<&str> = row.try_get("party_uid")?;
        let item: TransferItem = from_str(source).map_err(invalid)?;
        let terms = item
            .transport
            .ok_or_else(|| invalid("Transport terms unavailable"))?;
        terms.validate().map_err(invalid)?;
        Ok((
            terms,
            item.exchange
                .map(|route| route.giver)
                .or_else(|| party.map(str::to_owned)),
        ))
    })
    .fetch_optional(connection)
    .await
}

async fn offer_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Option<(String, Terms)>, StoreError> {
    let Some((mut terms, giver)) = terms_on(connection, transfer).await? else {
        return Ok(None);
    };
    let mut current = transfer.to_owned();
    let mut seen = BTreeSet::new();
    for _ in 0..8 {
        if seen.contains(&current) {
            return Err(invalid("Transport capacity references contain a cycle"));
        }
        let (source, parent): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT source_uid, parent_uid FROM transfer WHERE record_uid = ?")
                .bind(&current)
                .fetch_optional(&mut *connection)
                .await?
                .unwrap_or_default();
        let mut next = None;
        for candidate in source.into_iter().chain(parent) {
            if let Some((offer, driver)) = terms_on(connection, &candidate).await?
                && offer.direction == Direction::Offer
                && offer.activity == terms.activity
            {
                offer.segment(&terms).map_err(invalid)?;
                if giver != driver {
                    return Err(invalid("A booking must use the offering driver's capacity"));
                }
                next = Some((candidate, offer));
                break;
            }
        }
        let Some((uid, offer)) = next else {
            return Ok(Some((current, terms)));
        };
        seen.insert(current);
        current = uid;
        terms = offer;
    }
    Err(invalid("Transport capacity references exceed eight levels"))
}

struct Reservations {
    total: u32,
    segments: Vec<SegmentCapacity>,
}

impl Reservations {
    fn new(terms: &Terms) -> Self {
        let stops = terms.stops();
        let segments = stops
            .iter()
            .zip(stops.iter().skip(1))
            .map(|(&origin, &destination)| SegmentCapacity {
                origin: origin.into(),
                destination: destination.into(),
                reserved: 0,
                available: terms.capacity,
            })
            .collect();
        Self {
            total: terms.capacity,
            segments,
        }
    }

    fn reserve(&mut self, span: Range<usize>, count: u32) -> Result<(), StoreError> {
        let segments = self.segments.get_mut(span).ok_or_else(invalid_span)?;
        for segment in segments {
            segment.reserved = segment
                .reserved
                .checked_add(count)
                .ok_or_else(|| invalid("Transport reservation count overflows"))?;
            segment.available = self
                .total
                .checked_sub(segment.reserved)
                .ok_or_else(|| invalid("Transport capacity would be exceeded on this part"))?;
        }
        Ok(())
    }

    fn project(mut self, offer: String, span: Range<usize>) -> Result<Capacity, StoreError> {
        if self.segments.get(span.clone()).is_none() {
            return Err(invalid_span());
        }
        self.segments.truncate(span.end);
        self.segments.drain(..span.start);
        let reserved = self
            .segments
            .iter()
            .map(|segment| segment.reserved)
            .max()
            .unwrap_or(0);
        Ok(Capacity {
            offer,
            segments: self.segments,
            total: self.total,
            reserved,
            available: self.total - reserved,
        })
    }
}

fn invalid_span() -> StoreError {
    invalid("Transport part is outside its offering route")
}

async fn reservations_on(
    connection: &mut SqliteConnection,
) -> Result<BTreeMap<String, Reservations>, StoreError> {
    let rows = sqlx::query(
        "SELECT p.uid, p.transfer_uid, p.delta, p.item_json FROM promise p
         JOIN transfer t ON t.record_uid = p.transfer_uid
         JOIN record r ON r.uid = t.record_uid AND r.deleted_at IS NULL
         WHERE p.state IN ('agreed', 'active') AND json_extract(p.item_json, '$.transport.activity') IS NOT NULL
         ORDER BY p.transfer_uid, p.uid",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut reservations = BTreeMap::new();
    for promises in rows.chunk_by(|left, right| {
        left.get::<&str, _>("transfer_uid") == right.get::<&str, _>("transfer_uid")
    }) {
        let [first, ..] = promises else { continue };
        let transfer: &str = first.get("transfer_uid");
        if journey_on(connection, transfer)
            .await?
            .is_some_and(|journey| journey.capacity_released)
        {
            continue;
        }
        let mut exchanges = BTreeSet::new();
        let mut offer = None;
        for row in promises {
            let item: TransferItem = from_str(row.get::<&str, _>("item_json")).map_err(invalid)?;
            let exchange = item
                .exchange
                .map_or_else(|| row.get("uid"), |route| route.uid);
            if !exchanges.insert(exchange) {
                continue;
            }
            let booking = item
                .transport
                .ok_or_else(|| invalid("Booking terms unavailable"))?;
            if offer.is_none() {
                offer = offer_on(connection, transfer).await?;
            }
            let (uid, terms) = offer
                .as_ref()
                .ok_or_else(|| invalid("Capacity offer unavailable"))?;
            if row.get::<f64, _>("delta").abs() != f64::from(booking.count) {
                return Err(invalid("Booking quantity differs from its reviewed count"));
            }
            let span = terms.segment(&booking).map_err(invalid)?;
            reservations
                .entry(uid.clone())
                .or_insert_with(|| Reservations::new(terms))
                .reserve(span, booking.count)?;
        }
    }
    Ok(reservations)
}

/// Check all Transport capacities in the caller's snapshot.
///
/// # Errors
///
/// Returns an error if the journey evidence, booking terms, or projected reservations are invalid.
pub async fn validate_all_on(connection: &mut SqliteConnection) -> Result<(), StoreError> {
    reservations_on(connection).await.map(|_| ())
}

/// Read the capacity of the requested Transfer's part of its offering route.
///
/// # Errors
///
/// Returns an error if the route or booking terms are invalid or capacity cannot be verified.
pub async fn capacity(pool: &SqlitePool, transfer: &str) -> Result<Option<Capacity>, StoreError> {
    let mut tx = pool.begin().await?;
    let Some((offer, terms)) = offer_on(&mut tx, transfer).await? else {
        tx.rollback().await?;
        return Ok(None);
    };
    let (booking, _) = terms_on(&mut tx, transfer)
        .await?
        .ok_or_else(|| invalid("Transport part unavailable"))?;
    let span = terms.segment(&booking).map_err(invalid)?;
    let capacity = reservations_on(&mut tx)
        .await?
        .remove(&offer)
        .unwrap_or_else(|| Reservations::new(&terms))
        .project(offer, span)?;
    tx.rollback().await?;
    Ok(Some(capacity))
}
