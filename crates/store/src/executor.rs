//! Which Cell does a piece of recurring work (Ontology C7).
//!
//! Not a Karma concept, though Karma was the first caller. The question "when
//! an Organ has several Cells, which one of them actually does this?" is asked
//! by every scheduler that reaches outward, and the answer has to be the same
//! shape for all of them — one mechanism to reason about, one place to look
//! when something is not running. Today it governs Karma Programs (set in the
//! Karma sand's "Where rules run") and transfer delivery retries (set in the
//! Transfer's "Delivering Cell"); the test for whether a scheduler belongs is
//! whether a SECOND Cell doing the work costs somebody else something. Pruning
//! and Organ
//! polling fail that test — both are per-Cell work on this Cell's own disk and
//! own inbox — and deliberately do not use this.
//!
//! Note this is the SHARED half of C7. The other axis, whether THIS Cell runs a
//! given rule, is `karma::execution` and never travels; the two are independent
//! vetoes, ANDed, with no precedence between them.

use sqlx::SqlitePool;

use crate::StoreError;

/// Where the designated executor is written.
///
/// A Record extension, so it SYNCS and every Cell learns the same answer, and
/// per-key so two Cells designating different Records never clobber each other.
///
/// The name is deliberately about SCHEDULING rather than about Karma: it is
/// read off a Transfer as readily as off a Program, and a namespace saying
/// `karma` on a Transfer Record would send the next reader looking for a rule
/// that does not exist. It also carries two dots, which the wire's
/// `rsplit_once` field split depends on — see
/// `engine/tests/sync_ops.rs::the_executor_designation_survives_the_wire`.
pub const NAMESPACE: &str = "lince.schedule.executor";

/// Name the Cell that does this Record's recurring work, or clear it.
///
/// **Designated executor with MANUAL takeover, decided 2026-08-14** — not a
/// heartbeat lease. Automatic failover, exactly-once, and a transport where
/// Cells are routinely unreachable are three properties that cannot hold
/// together, and in this project's actual shape (an always-on Cell plus a
/// laptop offline half the time) heartbeat-and-expiry hands the lease to the
/// laptop WHENEVER IT MERELY CANNOT SEE the VPS — which is most of the time.
/// That produces duplicate Records and duplicate Transfers precisely in the
/// steady state, which is worse than having no lease at all.
///
/// So it is a value, not a lock: a last-writer-wins register that converges by
/// construction rather than a claim that must be renewed. Moving it is a
/// deliberate act by a person who knows the old Cell is gone. The cost is
/// stated rather than hidden — if the designated Cell is off, the work does not
/// happen, and that is a visible silence rather than a silent duplicate.
pub async fn designate(
    pool: &SqlitePool,
    record_uid: &str,
    cell_uid: Option<&str>,
) -> Result<(), StoreError> {
    crate::records::set_extension(
        pool,
        record_uid,
        NAMESPACE,
        &serde_json::json!({ "cell": cell_uid }),
    )
    .await
}

/// Which Cell is designated, if any. `None` means every Cell may do it.
pub async fn designated(pool: &SqlitePool, record_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(crate::records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|fds| {
            fds.get("cell")
                .and_then(|cell| cell.as_str().map(str::to_string))
        }))
}

/// Whether THIS Cell is the one to do `record_uid`'s recurring work.
///
/// Absence means yes, matching the local axis: an undesignated Record is done
/// by whoever holds it, so a designation that never arrives fails toward the
/// work happening rather than toward nothing happening anywhere.
///
/// Once a designation EXISTS the direction reverses, and deliberately: a Cell
/// that cannot identify itself is not the designated one. Designating is an
/// explicit act meaning "exactly one Cell", and a Cell guessing that it might
/// be the one is how the duplicate this exists to prevent gets made. That state
/// should not occur — `cells::ensure_local` runs at startup — which is why
/// answering it strictly costs nothing real.
pub async fn runs_here(pool: &SqlitePool, record_uid: &str) -> Result<bool, StoreError> {
    let Some(designated_cell) = designated(pool, record_uid).await? else {
        return Ok(true);
    };
    Ok(crate::cells::local(pool)
        .await?
        .is_some_and(|cell| cell.uid == designated_cell))
}
