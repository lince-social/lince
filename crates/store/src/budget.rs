//! Storage budgets (Ontology C2c): one stated ceiling per Cell, divided into
//! per-area shares, each area evicting inside its own share.
//!
//! **Why shares rather than one global cap.** A single cap does not allocate,
//! it races: whichever area grows fastest evicts everything else. Importing a
//! folder of photos would silently empty the quarantine ring, and one large
//! Facade fetch would push out the media a Record's body points at. With a
//! share each, an area filling up hurts that area and nothing else — which is
//! the only version of a budget an owner can reason about.
//!
//! **Why the shares are constants and only the total is configurable.** The
//! owner's decision is how much of their disk Lince may use. How Lince divides
//! it internally is Lince's job, and three sliders nobody understands turn a
//! budget into an unanswerable question instead of an answer to one.
//!
//! **What is NOT budgeted, said out loud.** The SQLite database — Records,
//! assertions, the op log — is not under the ceiling, and neither are published
//! DNA sand packages. Both are the owner's own data rather than a cache of
//! somebody else's, and evicting them would be deleting work, not reclaiming
//! space. The op log is bounded by its own mechanism (snapshot plus
//! checkpoint-gated pruning, C2) rather than by a byte ceiling. `Usage` reports
//! them anyway under `unbudgeted`, because a total that does not add up to what
//! `du` says is the failure mode where the surface loses the owner's trust the
//! first time they check it.

use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// 2 GiB. A default has to be a real number rather than "unlimited", or the
/// budget is off until somebody discovers the setting — which is every Cell.
pub const DEFAULT_TOTAL_BYTES: i64 = 2 * 1024 * 1024 * 1024;

/// An area of disk that holds OTHER people's bytes, and can therefore be
/// evicted without losing anything the owner made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// Images a Record body points at. Largest share: it is the one an owner
    /// fills deliberately, and the one whose loss is most visible.
    Media,
    /// Fetched Facade pages. Does not exist yet — it arrives with C9 — and its
    /// share is reserved rather than guessed at later, so the arithmetic below
    /// never has to change when the consumer lands.
    Facade,
    /// Rejected ops kept as evidence. Smallest share by a wide margin: it is a
    /// diagnostic, and a diagnostic that can consume a fifth of the ceiling is
    /// a denial-of-service surface rather than a diagnostic.
    Quarantine,
}

impl Area {
    /// This area's percentage of the total. They sum to 100 and a test says so
    /// — a set of shares that quietly summed to 90 would under-use the disk by
    /// a tenth and nothing would ever report it.
    pub const fn share_percent(self) -> i64 {
        match self {
            Self::Media => 70,
            Self::Facade => 25,
            Self::Quarantine => 5,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Facade => "facade",
            Self::Quarantine => "quarantine",
        }
    }

    pub const ALL: [Self; 3] = [Self::Media, Self::Facade, Self::Quarantine];
}

/// This area's ceiling in bytes, or `None` when the total is unlimited.
///
/// Integer arithmetic on purpose. A percentage of a byte count is exact in
/// `i64` and approximate in `f64`, and a ceiling that drifts by a rounding
/// error is a ceiling two Cells can disagree about.
#[must_use]
pub fn share(total_bytes: i64, area: Area) -> Option<i64> {
    (total_bytes > 0).then(|| total_bytes / 100 * area.share_percent())
}

/// The Cell's stated total, `0` meaning unlimited.
pub async fn total(pool: &SqlitePool) -> Result<i64, StoreError> {
    crate::config::ensure_default(pool).await?;
    Ok(
        sqlx::query("SELECT storage_budget_bytes FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?
            .get("storage_budget_bytes"),
    )
}

/// Set the Cell's total. Negative is refused rather than clamped: a caller
/// asking for -1 has a bug, and silently reading it as "unlimited" would hide
/// the bug behind a working-looking setting.
pub async fn set_total(pool: &SqlitePool, bytes: i64) -> Result<(), StoreError> {
    if bytes < 0 {
        return Err(sqlx::Error::Protocol(
            "storage budget cannot be negative (0 means unlimited)".into(),
        ));
    }
    crate::config::ensure_default(pool).await?;
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("UPDATE configuration SET storage_budget_bytes = ? WHERE id = 1")
        .bind(bytes)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Which entries to drop so one source's bytes fit inside its quota.
///
/// **Pure, and separate from every table it will be used on.** Quarantine uses
/// it today; the Facade cache uses the same function when C9 builds it. A
/// second implementation is how the two areas end up with two eviction
/// policies, and the difference would only ever be discovered as a bug.
///
/// `entries` is NEWEST FIRST, and newest is what survives. For quarantine that
/// is the property that matters: the evidence a peer is misbehaving right now
/// outranks the evidence it misbehaved last month. An entry larger than the
/// whole quota is dropped rather than kept — keeping it would mean one oversized
/// item permanently occupying an area budgeted for many.
#[must_use]
pub fn evict_plan<T: Copy>(entries: &[(T, i64)], quota: i64) -> Vec<T> {
    if quota <= 0 {
        return entries.iter().map(|(id, _)| *id).collect();
    }
    let mut kept = 0_i64;
    let mut evicted = Vec::new();
    for (id, bytes) in entries {
        // Saturating, because a corrupt row reporting a huge length must evict
        // rather than wrap negative and look like free space.
        let next = kept.saturating_add(*bytes);
        if next > quota {
            evicted.push(*id);
        } else {
            kept = next;
        }
    }
    evicted
}

/// One area's used and allowed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaUsage {
    pub area: Area,
    pub used_bytes: i64,
    /// `None` when the total is unlimited.
    pub limit_bytes: Option<i64>,
    /// Whether this area has a consumer yet. The Facade cache does not until
    /// C9, and a row reading `0 B` for that reason must not look like a row
    /// reading `0 B` because nothing is stored — the empty state has to say
    /// WHICH nothing it means.
    pub live: bool,
}

/// What the whole Cell is holding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// `0` means unlimited.
    pub total_bytes: i64,
    pub areas: Vec<AreaUsage>,
    /// Bytes Lince holds that no share covers — the database and published DNA
    /// packages. Reported, never evicted.
    pub unbudgeted_bytes: i64,
}

impl Usage {
    /// Everything Lince is holding, budgeted or not. This is the number an
    /// owner compares against their file manager, so it has to be the honest
    /// one rather than the flattering one.
    #[must_use]
    pub fn on_disk_bytes(&self) -> i64 {
        self.areas
            .iter()
            .map(|a| a.used_bytes)
            .sum::<i64>()
            .saturating_add(self.unbudgeted_bytes)
    }
}

/// Total bytes of quarantined payloads.
pub async fn quarantine_bytes(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(SUM(LENGTH(payload)), 0) AS n FROM sync_quarantine")
            .fetch_one(pool)
            .await?
            .get("n"),
    )
}

/// The database file's own size, as SQLite reports it.
pub async fn database_bytes(pool: &SqlitePool) -> Result<i64, StoreError> {
    // `page_count * page_size` rather than a filesystem stat: it works for an
    // in-memory pool, which is what most tests run on, and it is what SQLite
    // itself considers the database to be.
    let pages: i64 = sqlx::query("PRAGMA page_count")
        .fetch_one(pool)
        .await?
        .get(0);
    let size: i64 = sqlx::query("PRAGMA page_size")
        .fetch_one(pool)
        .await?
        .get(0);
    Ok(pages.saturating_mul(size))
}

/// Assemble the report. Disk-backed areas are measured by the caller, because
/// where those directories live is the frontend's layout and not the store's
/// to know — passing the number in keeps the policy here and the paths there.
///
/// `facade_bytes` is `None` until C9 gives the Facade cache a consumer, and
/// that `None` is what makes the surface able to say "not built yet" instead of
/// showing a confident zero.
pub async fn usage(
    pool: &SqlitePool,
    media_bytes: i64,
    dna_bytes: i64,
    facade_bytes: Option<i64>,
) -> Result<Usage, StoreError> {
    let total_bytes = total(pool).await?;
    let areas = vec![
        AreaUsage {
            area: Area::Media,
            used_bytes: media_bytes,
            limit_bytes: share(total_bytes, Area::Media),
            live: true,
        },
        AreaUsage {
            area: Area::Facade,
            used_bytes: facade_bytes.unwrap_or(0),
            limit_bytes: share(total_bytes, Area::Facade),
            live: facade_bytes.is_some(),
        },
        AreaUsage {
            area: Area::Quarantine,
            used_bytes: quarantine_bytes(pool).await?,
            limit_bytes: share(total_bytes, Area::Quarantine),
            live: true,
        },
    ];
    Ok(Usage {
        total_bytes,
        areas,
        unbudgeted_bytes: database_bytes(pool).await?.saturating_add(dna_bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shares_account_for_the_whole_budget() {
        let sum: i64 = Area::ALL.iter().map(|a| a.share_percent()).sum();
        assert_eq!(
            sum, 100,
            "shares that do not sum to 100 waste the difference"
        );
    }

    #[test]
    fn an_unlimited_total_gives_every_area_no_ceiling() {
        for area in Area::ALL {
            assert_eq!(share(0, area), None);
        }
    }

    #[test]
    fn eviction_keeps_the_newest_and_drops_what_will_not_fit() {
        // Newest first. 40 + 40 fits in 100; the third would take it to 120.
        let entries = [("new", 40), ("mid", 40), ("old", 40)];
        assert_eq!(evict_plan(&entries, 100), vec!["old"]);
    }

    #[test]
    fn an_entry_larger_than_the_whole_quota_is_dropped_not_kept() {
        // Otherwise one oversized item occupies an area budgeted for many, and
        // every later entry is evicted around it forever.
        let entries = [("huge", 500), ("small", 10)];
        assert_eq!(evict_plan(&entries, 100), vec!["huge"]);
    }

    #[test]
    fn a_zero_quota_keeps_nothing() {
        let entries = [("a", 1), ("b", 1)];
        assert_eq!(evict_plan(&entries, 0), vec!["a", "b"]);
    }

    #[test]
    fn a_corrupt_length_evicts_rather_than_wrapping_into_free_space() {
        let entries = [("sane", 10), ("corrupt", i64::MAX), ("later", 10)];
        assert_eq!(evict_plan(&entries, 100), vec!["corrupt"]);
    }
}
