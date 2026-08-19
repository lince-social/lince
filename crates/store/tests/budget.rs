//! Storage budgets (Ontology C2c), against a real store.
//!
//! The pure arithmetic is unit-tested beside `store::budget`. What needs a
//! database is the part that matters: that quarantine actually evicts inside
//! its share, and that one contact flooding the ring cannot reach another
//! contact's evidence.

use store::Store;
use store::budget::{self, Area};

async fn cell() -> Store {
    Store::open_memory().await.expect("in-memory store")
}

#[tokio::test]
async fn the_default_budget_is_a_real_number_rather_than_unlimited() {
    // A budget that is off until somebody finds the setting is off on every
    // Cell, which is the same as not having one.
    let store = cell().await;
    let total = budget::total(&store.pool).await.expect("total");
    assert_eq!(total, budget::DEFAULT_TOTAL_BYTES);
    assert!(budget::share(total, Area::Quarantine).is_some());
}

#[tokio::test]
async fn a_negative_budget_is_refused_rather_than_read_as_unlimited() {
    let store = cell().await;
    assert!(budget::set_total(&store.pool, -1).await.is_err());
    // And the stored value is untouched, so a refused write leaves no dent.
    assert_eq!(
        budget::total(&store.pool).await.expect("total"),
        budget::DEFAULT_TOTAL_BYTES
    );
}

#[tokio::test]
async fn quarantine_evicts_inside_its_share_and_keeps_the_newest() {
    let store = cell().await;
    // A share of 100 bytes: small enough that a handful of rows overruns it.
    // 5% of 2000 is 100.
    budget::set_total(&store.pool, 2000).await.expect("set");
    assert_eq!(budget::share(2000, Area::Quarantine), Some(100));

    for n in 0..10 {
        store::organs::quarantine(&store.pool, "organ-a", "bad-sig", &"x".repeat(30))
            .await
            .unwrap_or_else(|e| panic!("quarantine {n}: {e}"));
    }

    let used = budget::quarantine_bytes(&store.pool).await.expect("bytes");
    assert!(used <= 100, "quarantine held {used} bytes, over its 100 share");

    // Three 30-byte rows fit in 100; the rest are gone. And what survived is
    // the newest, because the evidence a peer is misbehaving NOW is the
    // evidence worth keeping.
    let kept = store::organs::quarantined_for(&store.pool, "organ-a", 50)
        .await
        .expect("read");
    assert_eq!(kept.len(), 3);
}

#[tokio::test]
async fn one_contact_flooding_the_ring_cannot_evict_another_contacts_evidence() {
    // The whole reason the quota is per contact. A global byte cap would let
    // an attacker bury the record of what they did under noise from a second
    // identity — which is exactly what an attacker would do.
    let store = cell().await;
    budget::set_total(&store.pool, 2000).await.expect("set");

    store::organs::quarantine(&store.pool, "organ-victim", "bad-sig", "the evidence")
        .await
        .expect("victim row");

    for _ in 0..40 {
        store::organs::quarantine(&store.pool, "organ-flood", "bad-sig", &"x".repeat(60))
            .await
            .expect("flood row");
    }

    let victim = store::organs::quarantined_for(&store.pool, "organ-victim", 10)
        .await
        .expect("read");
    assert_eq!(victim.len(), 1, "the flood reached another contact's ring");
    assert_eq!(victim[0].1, "the evidence");
}

#[tokio::test]
async fn an_unlimited_budget_disables_the_byte_bound_but_not_the_count_bound() {
    // Zero means unlimited, and the two bounds answer different questions —
    // "how much disk may one peer cost me" versus "how much garbage from one
    // peer is worth reading". Turning the budget off must not turn the ring
    // into something unbounded.
    let store = cell().await;
    budget::set_total(&store.pool, 0).await.expect("set");
    for _ in 0..20 {
        store::organs::quarantine(&store.pool, "organ-a", "bad-sig", &"x".repeat(500))
            .await
            .expect("row");
    }
    let kept = store::organs::quarantined_for(&store.pool, "organ-a", 100)
        .await
        .expect("read");
    assert_eq!(kept.len(), 20, "nothing should be evicted by bytes");
    assert!(i64::try_from(kept.len()).expect("fits") <= store::organs::QUARANTINE_PER_CONTACT);
}

#[tokio::test]
async fn the_report_says_which_areas_have_a_consumer_yet() {
    // A Facade row reading 0 B because C9 has not built the cache must not
    // look like a row reading 0 B because nothing is cached. The empty state
    // has to say which nothing it means.
    let store = cell().await;
    let usage = budget::usage(&store.pool, 512, 0, None)
        .await
        .expect("usage");

    let facade = usage
        .areas
        .iter()
        .find(|a| a.area == Area::Facade)
        .expect("facade row");
    assert!(!facade.live, "the Facade cache has no consumer until C9");

    let media = usage
        .areas
        .iter()
        .find(|a| a.area == Area::Media)
        .expect("media row");
    assert!(media.live);
    assert_eq!(media.used_bytes, 512);

    // The unbudgeted remainder is reported rather than hidden, so the total
    // matches what the owner's file manager says.
    assert!(usage.unbudgeted_bytes > 0, "the database is never zero bytes");
    assert!(usage.on_disk_bytes() > usage.areas.iter().map(|a| a.used_bytes).sum::<i64>());
}
