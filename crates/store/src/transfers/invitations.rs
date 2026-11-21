use chrono::{DateTime, Utc};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};

use crate::StoreError;

pub(super) mod events;
pub(super) mod legacy;
pub(super) mod lifecycle;

/// The current state of an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferInvitationStatus {
    Pending,
    Accepted,
    Rejected,
    Withdrawn,
    Expired,
}

impl TransferInvitationStatus {
    /// Returns the stored and signed status spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
            Self::Expired => "expired",
        }
    }

    fn from_db(value: &str) -> Result<Self, StoreError> {
        match value {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "withdrawn" => Ok(Self::Withdrawn),
            "expired" => Ok(Self::Expired),
            _ => Err(sqlx::Error::Protocol(format!(
                "unknown transfer invitation status `{value}`"
            ))),
        }
    }
}

/// A stored invitation, including its latest attempt and resulting party.
#[derive(Debug, Clone)]
pub struct TransferInvitationRow {
    pub uid: String,
    pub transfer_uid: String,
    pub addressed_person_uid: String,
    pub invited_by_person_uid: String,
    pub status: TransferInvitationStatus,
    pub attempt: u64,
    pub party_uid: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn map_invitation(row: SqliteRow) -> Result<TransferInvitationRow, StoreError> {
    Ok(TransferInvitationRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        addressed_person_uid: row.get("addressed_person_uid"),
        invited_by_person_uid: row.get("invited_by_person_uid"),
        status: TransferInvitationStatus::from_db(row.get("status"))?,
        attempt: row.get::<i64, _>("attempt").cast_unsigned(),
        party_uid: row.get("party_uid"),
        expires_at: row.get("expires_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

/// Reads an invitation by UID.
///
/// # Errors
///
/// Returns an error if storage fails or the status is invalid.
pub async fn invitation(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map_invitation)
        .transpose()
}

/// Reads invitations in creation-time and UID order.
///
/// # Errors
///
/// Returns an error if storage fails or a status is invalid.
pub async fn invitations_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

/// Reads the Person's pending invitations in creation-time and UID order.
///
/// # Errors
///
/// Returns an error if storage fails or a status is invalid.
pub async fn pending_invitations_for_person(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE addressed_person_uid = ? AND status = 'pending'
         ORDER BY created_at, uid",
    )
    .bind(person_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

/// Reads pending invitations whose deadlines have arrived, earliest first.
///
/// # Errors
///
/// Returns an error if storage fails or a status is invalid.
pub async fn due_transfer_invitations(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<TransferInvitationRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at <= ?
         ORDER BY expires_at, uid",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation)
    .collect()
}

impl TransferInvitationRow {
    fn expires_by(&self, now: DateTime<Utc>) -> bool {
        self.expires_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|value| value <= now)
    }
}

async fn pending_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ? AND status = 'pending'")
        .bind(uid)
        .fetch_optional(connection)
        .await?
        .map(map_invitation)
        .transpose()
}

async fn require_people(
    connection: &mut SqliteConnection,
    addressed: &str,
    inviter: &str,
) -> Result<(), StoreError> {
    for (role, uid) in [("addressed", addressed), ("inviting", inviter)] {
        let person: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND kind = 'person')",
        )
        .bind(uid)
        .fetch_one(&mut *connection)
        .await?;
        if !person {
            return Err(StoreError::Protocol(format!(
                "{role} transfer invitation identity `{uid}` is not a Person"
            )));
        }
    }
    Ok(())
}

async fn is_party(
    connection: &mut SqliteConnection,
    transfer_uid: &str,
    person_uid: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_party WHERE transfer_uid = ? AND actor_uid = ?)",
    )
    .bind(transfer_uid)
    .bind(person_uid)
    .fetch_one(connection)
    .await
}
