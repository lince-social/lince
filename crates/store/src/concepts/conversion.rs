use std::collections::HashSet;

use nucleus::{
    DecimalValue,
    karma::{RoundedDecimal, Rounding},
};
use sqlx::{Row, SqliteConnection, SqlitePool, query};

use super::hierarchy::ancestors_including_on;
use crate::{StoreError, exact::from_f64, write_tx};

/// Sets the exact positive conversion ratio, replacing a reverse entry if present.
///
/// # Errors
/// Returns an error for identical units, a nonpositive ratio, or a failed write.
pub async fn set_conversion_exact(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    numerator: i128,
    denominator: i128,
) -> Result<(), StoreError> {
    if a_uid == b_uid {
        return Err(StoreError::Protocol(
            "Choose distinct units for a conversion".into(),
        ));
    }
    if numerator <= 0 || denominator <= 0 {
        return Err(StoreError::Protocol(format!(
            "conversion ratio must be positive, got {numerator}/{denominator}"
        )));
    }
    let mut transaction = write_tx(pool).await?;
    query(
        "INSERT INTO concept_conversion (a_uid, b_uid, numerator, denominator)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(a_uid, b_uid) DO UPDATE
           SET numerator = excluded.numerator, denominator = excluded.denominator",
    )
    .bind(a_uid)
    .bind(b_uid)
    .bind(numerator.to_string())
    .bind(denominator.to_string())
    .execute(&mut *transaction)
    .await?;
    query("DELETE FROM concept_conversion WHERE a_uid = ? AND b_uid = ?")
        .bind(b_uid)
        .bind(a_uid)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

/// Sets a conversion from a finite positive floating-point factor.
///
/// # Errors
/// Returns an error for an invalid factor, identical units, or a failed write.
pub async fn set_conversion(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    factor: f64,
) -> Result<(), StoreError> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(StoreError::Protocol(format!(
            "conversion factor must be finite and positive, got {factor}"
        )));
    }
    let exact = from_f64(factor);
    let denominator = 10_i128
        .checked_pow(u32::from(exact.scale()))
        .ok_or_else(|| StoreError::Protocol("conversion factor scale overflows".into()))?;
    set_conversion_exact(pool, a_uid, b_uid, exact.mantissa(), denominator).await
}

/// Converts approximately between units that share a dimension and have an active ratio.
///
/// # Errors
/// Returns an error if the ratio or its referenced hierarchy cannot be read.
pub async fn convert(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
    quantity: f64,
) -> Result<Option<f64>, StoreError> {
    if from_uid == to_uid {
        return Ok(Some(quantity));
    }
    let Some((numerator, denominator)) = conversion_ratio(pool, from_uid, to_uid).await? else {
        return Ok(None);
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "This approximate conversion returns f64 and the exact API preserves precision"
    )]
    Ok(Some(quantity * (numerator as f64) / (denominator as f64)))
}

/// Converts at the requested scale and rounding mode using the exact stored ratio.
///
/// # Errors
/// Returns an error for invalid stored data, failed reads, or a conversion overflow.
pub async fn convert_exact(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
    amount: DecimalValue,
    scale: u8,
    rounding: Rounding,
) -> Result<Option<RoundedDecimal>, StoreError> {
    if from_uid == to_uid {
        return Ok(amount
            .rescale(scale)
            .map(|value| RoundedDecimal { value, exact: true })
            .or_else(|| amount.mul_ratio(1, 1, scale, rounding)));
    }
    let Some((numerator, denominator)) = conversion_ratio(pool, from_uid, to_uid).await? else {
        return Ok(None);
    };
    amount
        .mul_ratio(numerator, denominator, scale, rounding)
        .map(Some)
        .ok_or_else(|| {
            StoreError::Protocol(format!(
                "converting {from_uid} -> {to_uid} overflows the exact range"
            ))
        })
}

async fn conversion_ratio(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
) -> Result<Option<(i128, i128)>, StoreError> {
    conversion_ratio_on(&mut *pool.acquire().await?, from_uid, to_uid).await
}

pub(crate) async fn conversion_ratio_on(
    connection: &mut SqliteConnection,
    from_uid: &str,
    to_uid: &str,
) -> Result<Option<(i128, i128)>, StoreError> {
    let ratio = if let Some(direct) = read_ratio(connection, from_uid, to_uid).await? {
        direct
    } else if let Some((numerator, denominator)) = read_ratio(connection, to_uid, from_uid).await? {
        (denominator, numerator)
    } else {
        return Ok(None);
    };

    let from_dimension: HashSet<String> = ancestors_including_on(connection, from_uid)
        .await?
        .into_iter()
        .collect();
    let shares_dimension = ancestors_including_on(connection, to_uid)
        .await?
        .into_iter()
        .any(|ancestor| from_dimension.contains(&ancestor));
    Ok(shares_dimension.then_some(ratio))
}

async fn read_ratio(
    connection: &mut SqliteConnection,
    a_uid: &str,
    b_uid: &str,
) -> Result<Option<(i128, i128)>, StoreError> {
    let Some(row) = query(
        "SELECT c.numerator,c.denominator FROM concept_conversion c JOIN concept_conversion_version v ON v.a_uid=c.a_uid AND v.b_uid=c.b_uid AND v.revision=(SELECT MAX(revision) FROM concept_conversion_version WHERE a_uid=c.a_uid AND b_uid=c.b_uid) WHERE c.a_uid=? AND c.b_uid=? AND v.withdrawn=0 AND v.a_revision=COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=c.a_uid),1) AND v.b_revision=COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=c.b_uid),1)",
    )
    .bind(a_uid)
    .bind(b_uid)
    .fetch_optional(&mut *connection)
    .await?
    else {
        return Ok(None);
    };
    let parse = |value: &str| {
        value.parse::<i128>().map_err(|error| {
            StoreError::Protocol(format!(
                "conversion ratio {value:?} is not an integer: {error}"
            ))
        })
    };
    Ok(Some((
        parse(row.try_get("numerator")?)?,
        parse(row.try_get("denominator")?)?,
    )))
}
