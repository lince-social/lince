use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    ThreadInvite,
    ReplicaGrant,
    RecordMove,
    Transfer,
}

impl OfferKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OfferKind::ThreadInvite => "thread-invite",
            OfferKind::ReplicaGrant => "replica-grant",
            OfferKind::RecordMove => "record-move",
            OfferKind::Transfer => "transfer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Incoming => "incoming",
            Direction::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Offer {
    pub kind: OfferKind,
    pub direction: Direction,
    pub subject_uid: String,
    pub title: String,
    pub other_party: String,
    pub created_at: String,
    pub previously_refused: bool,
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<Offer>, StoreError> {
    let mut out = Vec::new();

    for invite in crate::invites::pending(pool).await? {
        out.push(Offer {
            kind: OfferKind::ThreadInvite,
            direction: Direction::Incoming,
            subject_uid: invite.root,
            title: invite.title,
            other_party: invite.from_organ,
            created_at: invite.created_at,
            previously_refused: false,
        });
    }

    let grants = sqlx::query(
        "SELECT g.root_record, g.contact_organ, g.created_at,
                COALESCE(r.head, '') AS head
           FROM replica_grant g
           LEFT JOIN record r ON r.uid = g.root_record
          WHERE g.state = ?
          ORDER BY g.created_at, g.root_record",
    )
    .bind(crate::replica::OFFERED)
    .fetch_all(pool)
    .await?;
    for row in grants {
        out.push(Offer {
            kind: OfferKind::ReplicaGrant,
            direction: Direction::Outgoing,
            subject_uid: row.get("root_record"),
            title: row.get("head"),
            other_party: row.get("contact_organ"),
            created_at: row.get("created_at"),
            previously_refused: false,
        });
    }

    for moving in crate::record_move::pending(pool).await? {
        let title: Option<String> = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
            .bind(&moving.record_uid)
            .fetch_optional(pool)
            .await?;
        out.push(Offer {
            kind: OfferKind::RecordMove,
            direction: Direction::Outgoing,
            subject_uid: moving.record_uid,
            title: title.unwrap_or_default(),
            other_party: moving.contact_organ,
            created_at: moving.started_at,
            previously_refused: false,
        });
    }

    let transfers = sqlx::query(
        "SELECT i.uid, i.transfer_uid, i.created_at,
                COALESCE(i.addressed_person_uid, '') AS addressee,
                COALESCE(r.head, '') AS head
           FROM transfer_invitation i
           LEFT JOIN record r ON r.uid = i.transfer_uid
          WHERE i.status = 'pending'
          ORDER BY i.created_at, i.uid",
    )
    .fetch_all(pool)
    .await?;
    for row in transfers {
        out.push(Offer {
            kind: OfferKind::Transfer,
            direction: Direction::Incoming,
            subject_uid: row.get("transfer_uid"),
            title: row.get("head"),
            other_party: row.get("addressee"),
            created_at: row.get("created_at"),
            previously_refused: false,
        });
    }

    for offer in &mut out {
        offer.previously_refused =
            standing_refusal(pool, offer.kind, &offer.subject_uid, &offer.other_party).await?;
    }

    out.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.subject_uid.cmp(&right.subject_uid))
    });
    Ok(out)
}

pub const REFUSAL_WINDOW_DAYS: i64 = 30;

#[derive(Debug, Clone)]
pub struct Refusal {
    pub kind: String,
    pub subject_uid: String,
    pub other_party: String,
    pub at: String,
    pub until: String,
}

pub async fn refuse(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<(), StoreError> {
    let now = chrono::Utc::now();
    let until = now + chrono::Duration::days(REFUSAL_WINDOW_DAYS);
    sqlx::query(
        "INSERT INTO offer_refusal (kind, subject_uid, other_party, at, until)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(kind, subject_uid, other_party)
         DO UPDATE SET at = excluded.at, until = excluded.until",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .bind(now.to_rfc3339())
    .bind(until.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn standing_refusal(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<bool, StoreError> {
    let until: Option<String> = sqlx::query_scalar(
        "SELECT until FROM offer_refusal
          WHERE kind = ? AND subject_uid = ? AND other_party = ?",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .fetch_optional(pool)
    .await?;
    let Some(until) = until else {
        return Ok(false);
    };
    Ok(chrono::DateTime::parse_from_rfc3339(&until)
        .map(|when| when.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or(false))
}

pub async fn refused_by_party(
    pool: &SqlitePool,
    kind: OfferKind,
    other_party: &str,
) -> Result<bool, StoreError> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT until FROM offer_refusal WHERE kind = ? AND other_party = ?")
            .bind(kind.as_str())
            .bind(other_party)
            .fetch_all(pool)
            .await?;
    let now = chrono::Utc::now();
    Ok(rows.iter().any(|until| {
        chrono::DateTime::parse_from_rfc3339(until)
            .map(|when| when.with_timezone(&chrono::Utc) > now)
            .unwrap_or(false)
    }))
}

pub async fn refusals(pool: &SqlitePool) -> Result<Vec<Refusal>, StoreError> {
    Ok(sqlx::query(
        "SELECT kind, subject_uid, other_party, at, until FROM offer_refusal
          ORDER BY at DESC, subject_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| Refusal {
        kind: row.get("kind"),
        subject_uid: row.get("subject_uid"),
        other_party: row.get("other_party"),
        at: row.get("at"),
        until: row.get("until"),
    })
    .collect())
}

pub async fn forget_refusal(
    pool: &SqlitePool,
    kind: OfferKind,
    subject_uid: &str,
    other_party: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM offer_refusal
          WHERE kind = ? AND subject_uid = ? AND other_party = ?",
    )
    .bind(kind.as_str())
    .bind(subject_uid)
    .bind(other_party)
    .execute(pool)
    .await?;
    Ok(())
}
