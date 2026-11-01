use nucleus::karma::{ReferenceKind, TypedUid};
use nucleus::projection::{Context, ExecutionIdentity, Scheduled, Window};
use nucleus::schedule::TimeRange;
use nucleus::simulation::{Cause, Quantity, RuleOccurrence};
use store::{Store, projection};

fn identity(digit: &str) -> &'static ExecutionIdentity {
    static FIRST: std::sync::LazyLock<ExecutionIdentity> = std::sync::LazyLock::new(|| {
        ExecutionIdentity::parse(format!("sha256:{}", "a".repeat(64))).unwrap()
    });
    static SECOND: std::sync::LazyLock<ExecutionIdentity> = std::sync::LazyLock::new(|| {
        ExecutionIdentity::parse(format!("sha256:{}", "b".repeat(64))).unwrap()
    });
    match digit {
        "a" => &FIRST,
        "b" => &SECOND,
        _ => panic!("unknown test identity"),
    }
}

#[tokio::test]
async fn projection_cache_requires_matching_explicit_identity_on_every_path() {
    let unbound = Store::open_memory().await.unwrap();
    let bound = unbound.clone().with_execution_identity(identity("a"));
    let other = unbound.clone().with_execution_identity(identity("b"));
    assert!(unbound.execution_identity().is_none());
    assert_eq!(
        bound.clone().execution_identity(),
        bound.execution_identity()
    );
    let now = 1_700_000_000_000;
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    let scheduled = Scheduled {
        id: "future-work".into(),
        record: TypedUid::new(ReferenceKind::Record, nucleus::new_uid("r")).unwrap(),
        time: TimeRange {
            from_ms: now + 1_000,
            until_ms: Some(now + 2_000),
        },
        quantity: Quantity {
            value: nucleus::DecimalValue::parse_inferred("-1").unwrap(),
            unit: None,
        },
        cause: Cause::Rule {
            occurrence: RuleOccurrence {
                rule_uid: nucleus::new_uid("r"),
                revision: 1,
                event_id: "event".into(),
                frequency: None,
                intended_at_ms: Some(now + 1_000),
            },
            consequence: 0,
        },
        head: "Future work".into(),
        slug: None,
        record_kind: "plain".into(),
        preview: false,
    };
    let revision = projection::revision(&bound.pool).await.unwrap();
    assert!(
        projection::publish_schedule(
            &bound,
            &context,
            revision,
            now,
            now + 500,
            None,
            &[],
            std::slice::from_ref(&scheduled)
        )
        .await
        .unwrap()
    );
    let read = projection::read(&bound, &context, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.schedule, vec![scheduled.clone()]);
    assert!(
        projection::covered_window(&bound, &context, now)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        projection::previous_schedule(&bound, &context, now + 750)
            .await
            .unwrap(),
        vec![scheduled.clone()]
    );
    for unavailable in [&unbound, &other] {
        assert!(
            projection::read(unavailable, &context, now)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            projection::covered_window(unavailable, &context, now)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            projection::previous_schedule(unavailable, &context, now + 750)
                .await
                .unwrap()
                .is_empty()
        );
    }
    let before: i64 = store::sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&bound.pool)
        .await
        .unwrap();
    assert!(
        projection::publish(&unbound, &context, revision, now, now + 500, None, &[])
            .await
            .is_err()
    );
    let after: i64 = store::sqlx::query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&bound.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        projection::read(&bound, &context, now)
            .await
            .unwrap()
            .unwrap()
            .schedule,
        vec![scheduled]
    );
    let mut restricted = context.clone();
    restricted.actor = Some("visitor".into());
    assert!(
        projection::read(&bound, &restricted, now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        projection::previous_schedule(&bound, &restricted, now + 750)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        projection::read(&bound, &context, now + 500)
            .await
            .unwrap()
            .is_none()
    );
    store::sqlx::query("UPDATE projection_source SET revision = revision + 1 WHERE id = 1")
        .execute(&bound.pool)
        .await
        .unwrap();
    assert!(
        projection::read(&bound, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        projection::covered_window(&bound, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !projection::publish(&bound, &context, revision, now, now + 500, None, &[])
            .await
            .unwrap()
    );
    bound.pool.close().await;
}

#[tokio::test]
async fn reopening_storage_does_not_implicitly_trust_a_persisted_projection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projection.sqlite");
    let url = format!("sqlite://{}", path.display());
    let original = Store::open(&url)
        .await
        .unwrap()
        .with_execution_identity(identity("a"));
    let now = 1_700_000_000_000;
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 1_000,
            timezone: "UTC".into(),
        },
    };
    let revision = projection::revision(&original.pool).await.unwrap();
    assert!(
        projection::publish(&original, &context, revision, now, now + 500, None, &[])
            .await
            .unwrap()
    );
    original.pool.close().await;
    let reopened = Store::open_existing_durable(&url).await.unwrap();
    assert!(
        projection::read(&reopened, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    let trusted = reopened.with_execution_identity(identity("a"));
    assert!(
        projection::read(&trusted, &context, now)
            .await
            .unwrap()
            .is_some()
    );
    let raw = Store::from_pool(trusted.pool.clone());
    assert!(
        projection::read(&raw, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    trusted.pool.close().await;
}
