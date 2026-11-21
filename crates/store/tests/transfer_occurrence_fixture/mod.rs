use store::{Store, StoreError, sqlx::raw_sql};

pub async fn pending_transfer() -> Result<Store, StoreError> {
    let store = Store::open_memory().await?;
    raw_sql(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at) VALUES
            ('transfer', 'transfer', 'organ', 'fixture', 'fixture'),
            ('giver', 'person', 'organ', 'fixture', 'fixture'),
            ('receiver', 'person', 'organ', 'fixture', 'fixture');
         INSERT INTO transfer (record_uid, revision) VALUES ('transfer', 1);
         INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES
            ('giver-party', 'transfer', 'giver'), ('receiver-party', 'transfer', 'receiver');
         INSERT INTO promise (uid, record_uid, delta, transfer_uid, created_at, updated_at)
            VALUES ('promise', 'transfer', 1, 'transfer', 'fixture', 'fixture'),
                   ('second-promise', 'transfer', 1, 'transfer', 'fixture', 'fixture')",
    )
    .execute(&store.pool)
    .await?;
    Ok(store)
}

pub async fn activated_transfer() -> Result<Store, StoreError> {
    let store = pending_transfer().await?;
    raw_sql(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, prev_hash, hash)
            VALUES ('activation-fact', 'transfer', '0', 0, '2026-01-01T00:00:00Z', 'settlement', '', 'fixture');
         INSERT INTO transfer_phase4_request (idempotency_key, kind, target_uid, fact_uid, created_at)
            VALUES ('activate', 'activation', 'transfer', 'activation-fact', 'fixture');
         INSERT INTO transfer_exchange_path
            (uid, transfer_uid, revision, primary_promise_uid, created_at)
            VALUES ('path', 'transfer', 1, 'promise', 'fixture'),
                   ('second-path', 'transfer', 1, 'second-promise', 'fixture');
         INSERT INTO transfer_activation_event
            (uid, transfer_uid, revision, actor_person_uid, fact_uid, idempotency_key, created_at)
            VALUES ('activation', 'transfer', 1, 'giver', 'activation-fact', 'activate', 'fixture');
         INSERT INTO transfer_occurrence
            (uid, transfer_uid, revision, promise_uid, exchange_path_uid, activation_event_uid,
             record_uid, quantity, giver_person_uid, receiver_person_uid, created_at)
            VALUES ('occurrence', 'transfer', 1, 'promise', 'path', 'activation',
                    'transfer', 1, 'giver', 'receiver', 'fixture'),
                   ('second-occurrence', 'transfer', 1, 'second-promise', 'second-path', 'activation',
                    'transfer', 1, 'giver', 'receiver', 'fixture');",
    )
    .execute(&store.pool)
    .await?;
    Ok(store)
}
