use nucleus::transfer::disclosure::TransferItem;
use sqlx::SqlitePool;

use super::TransferLocationSnapshot;
use crate::StoreError;

pub(super) fn validate_location(
    location: Option<&TransferLocationSnapshot>,
) -> Result<(), StoreError> {
    let Some(location) = location else {
        return Ok(());
    };
    if location.lat.is_some() != location.lon.is_some() {
        return Err(sqlx::Error::Protocol(
            "location latitude and longitude must be provided together".into(),
        ));
    }
    if let (Some(lat), Some(lon)) = (location.lat, location.lon)
        && (!lat.is_finite()
            || !lon.is_finite()
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lon))
    {
        return Err(sqlx::Error::Protocol(
            "location coordinates are outside valid latitude/longitude bounds".into(),
        ));
    }
    Ok(())
}

pub(super) fn encode_item(item: Option<&TransferItem>) -> Result<Option<String>, StoreError> {
    item.map(|item| {
        item.validate()
            .map_err(|error| sqlx::Error::Protocol(error.into()))?;
        serde_json::to_string(item).map_err(|error| sqlx::Error::Protocol(error.to_string()))
    })
    .transpose()
}

/// Reads the structured item for a promise.
///
/// # Errors
///
/// Returns an error if the item cannot be decoded or read.
pub async fn item_of(pool: &SqlitePool, promise: &str) -> Result<Option<TransferItem>, StoreError> {
    let json: Option<String> = sqlx::query_scalar("SELECT item_json FROM promise WHERE uid = ?")
        .bind(promise)
        .fetch_one(pool)
        .await?;
    decode_item(json)
}

pub(super) fn decode_item(json: Option<String>) -> Result<Option<TransferItem>, StoreError> {
    json.map(|value| {
        serde_json::from_str(&value).map_err(|error| sqlx::Error::Protocol(error.to_string()))
    })
    .transpose()
}

pub(super) fn location_snapshot(
    lat: Option<f64>,
    lon: Option<f64>,
    address: Option<String>,
) -> Option<TransferLocationSnapshot> {
    if lat.is_none() && lon.is_none() && address.is_none() {
        None
    } else {
        Some(TransferLocationSnapshot { lat, lon, address })
    }
}
