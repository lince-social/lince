use sqlx::{Row, SqlitePool, query};

use super::{Direction, Offer, OfferKind, standing_refusal};
use crate::{StoreError, invites, organs, record_move::offers::list, replica::OFFERED};

/// Lists pending offers by creation timestamp and subject identity.
/// # Errors
/// Returns an error if any offer source or standing refusal cannot be read.
pub async fn pending(pool: &SqlitePool) -> Result<Vec<Offer>, StoreError> {
    let mut out = Vec::new();

    if let Some(organ) = organs::local(pool).await? {
        let incoming = query("SELECT id, peer, label, created_at FROM blob_sync WHERE owner = ? AND direction = 'incoming' AND state = 'offered'")
            .bind(&organ.uid)
            .fetch_all(pool)
            .await?;
        for row in incoming {
            let label: String = row.try_get("label")?;
            out.push(Offer {
                kind: OfferKind::BlobSync,
                direction: Direction::Incoming,
                subject_uid: row.try_get("id")?,
                title: format!("File copy from {label}"),
                other_party: row.try_get("peer")?,
                created_at: row.try_get("created_at")?,
                previously_refused: false,
            });
        }
    }

    for invite in invites::pending(pool).await? {
        out.push(Offer {
            kind: OfferKind::ThreadInvite,
            direction: Direction::Incoming,
            subject_uid: invite.record_uid,
            title: invite.title,
            other_party: invite.from_organ,
            created_at: invite.created_at,
            previously_refused: false,
        });
    }

    let grants = query(
        "SELECT g.root_record, g.contact_organ, g.created_at,
                COALESCE(r.head, '') AS head
           FROM replica_grant g
           LEFT JOIN record r ON r.uid = g.root_record
          WHERE g.state = ?
          ORDER BY g.created_at, g.root_record",
    )
    .bind(OFFERED)
    .fetch_all(pool)
    .await?;
    for row in grants {
        out.push(Offer {
            kind: OfferKind::ReplicaGrant,
            direction: Direction::Outgoing,
            subject_uid: row.try_get("root_record")?,
            title: row.try_get("head")?,
            other_party: row.try_get("contact_organ")?,
            created_at: row.try_get("created_at")?,
            previously_refused: false,
        });
    }

    for moving in list(pool).await? {
        if !matches!(
            moving.state.as_str(),
            "offered" | "accepted" | "transferring" | "changed"
        ) {
            continue;
        }
        out.push(Offer {
            kind: OfferKind::RecordMove,
            direction: if moving.direction == "incoming" {
                Direction::Incoming
            } else {
                Direction::Outgoing
            },
            subject_uid: moving.uid,
            title: moving
                .preview
                .records
                .into_iter()
                .find(|r| r.uid == moving.root)
                .map(|record| record.title)
                .unwrap_or_default(),
            other_party: moving.peer,
            created_at: moving.created_at,
            previously_refused: false,
        });
    }

    let transfers = query(
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
            subject_uid: row.try_get("uid")?,
            title: row.try_get("head")?,
            other_party: row.try_get("addressee")?,
            created_at: row.try_get("created_at")?,
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
