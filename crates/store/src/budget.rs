use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const DEFAULT_TOTAL_BYTES: i64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Media,
    Facade,
    Quarantine,
}

impl Area {
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

#[must_use]
pub fn share(total_bytes: i64, area: Area) -> Option<i64> {
    (total_bytes > 0).then(|| total_bytes / 100 * area.share_percent())
}

pub async fn total(pool: &SqlitePool) -> Result<i64, StoreError> {
    crate::config::ensure_default(pool).await?;
    Ok(
        sqlx::query("SELECT storage_budget_bytes FROM configuration WHERE id = 1")
            .fetch_one(pool)
            .await?
            .get("storage_budget_bytes"),
    )
}

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

#[must_use]
pub fn evict_plan<T: Copy>(entries: &[(T, i64)], quota: i64) -> Vec<T> {
    if quota <= 0 {
        return entries.iter().map(|(id, _)| *id).collect();
    }
    let mut kept = 0_i64;
    let mut evicted = Vec::new();
    for (id, bytes) in entries {
        let next = kept.saturating_add(*bytes);
        if next > quota {
            evicted.push(*id);
        } else {
            kept = next;
        }
    }
    evicted
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaUsage {
    pub area: Area,
    pub used_bytes: i64,
    pub limit_bytes: Option<i64>,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub total_bytes: i64,
    pub areas: Vec<AreaUsage>,
    pub unbudgeted_bytes: i64,
}

impl Usage {
    #[must_use]
    pub fn on_disk_bytes(&self) -> i64 {
        self.areas
            .iter()
            .map(|a| a.used_bytes)
            .sum::<i64>()
            .saturating_add(self.unbudgeted_bytes)
    }
}

pub async fn quarantine_bytes(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(SUM(LENGTH(payload)), 0) AS n FROM sync_quarantine")
            .fetch_one(pool)
            .await?
            .get("n"),
    )
}

pub async fn database_bytes(pool: &SqlitePool) -> Result<i64, StoreError> {
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
        let entries = [("new", 40), ("mid", 40), ("old", 40)];
        assert_eq!(evict_plan(&entries, 100), vec!["old"]);
    }

    #[test]
    fn an_entry_larger_than_the_whole_quota_is_dropped_not_kept() {
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
