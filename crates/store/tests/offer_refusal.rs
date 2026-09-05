use store::Store;
use store::offers::{OfferKind, refusals, refuse, standing_refusal};

async fn store() -> Store {
    Store::open_memory().await.expect("store opens")
}

#[tokio::test]
async fn a_refusal_stands_for_its_window() {
    let store = store().await;
    assert!(
        !standing_refusal(&store.pool, OfferKind::ThreadInvite, "root", "them")
            .await
            .expect("read"),
        "nothing refused yet"
    );

    refuse(&store.pool, OfferKind::ThreadInvite, "root", "them")
        .await
        .expect("refuse");

    assert!(
        standing_refusal(&store.pool, OfferKind::ThreadInvite, "root", "them")
            .await
            .expect("read")
    );
}

#[tokio::test]
async fn a_refusal_is_remembered_per_kind_and_per_party() {
    let store = store().await;
    refuse(&store.pool, OfferKind::ThreadInvite, "root", "them")
        .await
        .expect("refuse");

    assert!(
        !standing_refusal(&store.pool, OfferKind::ThreadInvite, "root", "someone-else")
            .await
            .expect("read"),
        "refusing one party says nothing about another"
    );
    assert!(
        !standing_refusal(&store.pool, OfferKind::Transfer, "root", "them")
            .await
            .expect("read"),
        "and refusing a conversation is not refusing a transfer"
    );
}

#[tokio::test]
async fn refusing_twice_moves_the_window_rather_than_stacking() {
    let store = store().await;
    for _ in 0..3 {
        refuse(&store.pool, OfferKind::RecordMove, "rec", "them")
            .await
            .expect("refuse");
    }
    let all = refusals(&store.pool).await.expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].other_party, "them");
}

#[tokio::test]
async fn an_expired_refusal_no_longer_stands() {
    let store = store().await;
    refuse(&store.pool, OfferKind::ThreadInvite, "root", "them")
        .await
        .expect("refuse");
    sqlx::query("UPDATE offer_refusal SET until = ?")
        .bind("2000-01-01T00:00:00Z")
        .execute(&store.pool)
        .await
        .expect("expire it");

    assert!(
        !standing_refusal(&store.pool, OfferKind::ThreadInvite, "root", "them")
            .await
            .expect("read"),
        "a refusal that never expires turns one bad moment into a life sentence"
    );
}
