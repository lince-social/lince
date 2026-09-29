use cell::{ClientMessage, ServerMessage};
use engine::actions::Action;
use nucleus::karma::{CadenceStep, rule_field::RuleFieldInput};
use std::sync::atomic::Ordering;

async fn act(session: &mut cell::Session, action: Action) -> Option<String> {
    let replies = session
        .handle(ClientMessage::Act {
            id: nucleus::new_uid("request"),
            action,
        })
        .await;
    assert!(
        !replies
            .iter()
            .any(|reply| matches!(reply, ServerMessage::Error { .. })),
        "{replies:?}"
    );
    replies
        .into_iter()
        .find_map(|reply| match reply {
            ServerMessage::ActionOk { created, .. } => Some(created),
            _ => None,
        })
        .expect("Action acknowledgement")
}

fn rule(frequency: &str, target: &str) -> Action {
    Action::SaveKarmaRule {
        identity: None,
        rule: None,
        expected_revision: None,
        fields: [
            format!("freq(@{frequency}) * -1"),
            "<0".into(),
            format!("@{target}"),
        ]
        .map(|source| RuleFieldInput::Text { source }),
        request_id: nucleus::new_uid("request"),
    }
}

#[test]
fn normal_cell_runs_karma_and_serves_cached_future_and_manual_work_through_protein() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .unwrap()
                .block_on(check_live())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn check_live() {
    assert!(nucleus::execution::current().is_none());
    let directory = tempfile::tempdir().unwrap();
    let store = store::Store::open(&format!(
        "sqlite://{}",
        directory.path().join("lince.db").display()
    ))
    .await
    .unwrap();
    let organ = store::organs::ensure_local(&store.pool, "http://127.0.0.1:6174")
        .await
        .unwrap();
    store::records::set_extension_raw(
        &store.pool,
        &organ.uid,
        "lince.discovery",
        &serde_json::json!({"internet":false,"local":false,"direct":false}),
    )
    .await
    .unwrap();
    store.pool.close().await;
    let cell = cell::Cell::open(cell::CellOptions {
        data_dir: Some(directory.path().into()),
        peer_port: Some(0),
        ..Default::default()
    })
    .await
    .unwrap();
    let mut session = cell.runtime().local_session();
    let mut records = Vec::new();
    for slug in ["future-work", "scheduled-work", "live-work"] {
        records.push(
            act(
                &mut session,
                Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: nucleus::RecordKind::Plain,
                    head: slug.into(),
                    body: String::new(),
                    quantity: if slug == "scheduled-work" { -1.0 } else { 3.0 },
                },
            )
            .await
            .unwrap(),
        );
    }
    let now = chrono::Utc::now();
    let due = (now + chrono::TimeDelta::days(2))
        .format("%Y-%m-%d")
        .to_string();
    act(
        &mut session,
        Action::SetExtension {
            target: records[1].clone(),
            namespace: "work".into(),
            fds: serde_json::json!({"due":due}),
        },
    )
    .await;
    act(
        &mut session,
        Action::CreateFrequency {
            slug: "daily".into(),
            head: None,
            every: CadenceStep {
                days: 1,
                ..Default::default()
            },
            anchor_at: Some((now + chrono::TimeDelta::days(1)).to_rfc3339()),
            request_id: Some("live-daily".into()),
        },
    )
    .await;
    act(&mut session, rule("daily", "future-work")).await;
    let query: protein::Protein = serde_json::from_value(serde_json::json!({
        "source":"calendar",
        "where":[{"quantity_lt":"0"},{"projection_window":{"from_ms":now.timestamp_millis(),"until_ms":(now + chrono::TimeDelta::days(5)).timestamp_millis(),"timezone":"UTC"}}]
    })).unwrap();
    let initial = session
        .handle(ClientMessage::Subscribe {
            id: "calendar".into(),
            protein: query.clone(),
        })
        .await;
    assert!(
        !initial
            .iter()
            .any(|reply| matches!(reply, ServerMessage::Error { .. })),
        "{initial:?}"
    );
    let rows = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        'ready: loop {
            let replies = session.refresh().await;
            assert!(
                !replies
                    .iter()
                    .any(|reply| matches!(reply, ServerMessage::Error { .. })),
                "{replies:?}"
            );
            for reply in replies {
                if let ServerMessage::Update { id, rows } | ServerMessage::Snapshot { id, rows } =
                    reply
                    && id == "calendar"
                    && rows.iter().any(|row| {
                        row["kind"] == "projection-status" && row["status"]["kind"] == "ready"
                    })
                {
                    break 'ready rows;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(rows.iter().any(|row| row["record_uid"] == records[0]
        && row["origin"]["kind"] == "projection"
        && row["quantity"] == "-1"));
    assert!(rows.iter().any(|row| row["uid"] == records[1]
        && row["origin"]["kind"] == "manual"
        && row["due_date"] == due));
    assert_eq!(
        store::records::get(&cell.engine().store.pool, &records[0])
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "3"
    );
    let snapshots = cell
        .engine()
        .projection
        .metrics
        .snapshots
        .load(Ordering::Relaxed);
    let steps = cell
        .engine()
        .projection
        .metrics
        .steps
        .load(Ordering::Relaxed);
    let warmed = session
        .handle(ClientMessage::Subscribe {
            id: "calendar-warm".into(),
            protein: query,
        })
        .await;
    assert!(warmed.iter().any(|reply| matches!(reply, ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["origin"]["kind"] == "projection"))));
    assert_eq!(
        cell.engine()
            .projection
            .metrics
            .snapshots
            .load(Ordering::Relaxed),
        snapshots
    );
    assert_eq!(
        cell.engine()
            .projection
            .metrics
            .steps
            .load(Ordering::Relaxed),
        steps
    );
    act(
        &mut session,
        Action::CreateFrequency {
            slug: "pulse".into(),
            head: None,
            every: CadenceStep {
                days: 1,
                ..Default::default()
            },
            anchor_at: Some((chrono::Utc::now() + chrono::TimeDelta::seconds(3)).to_rfc3339()),
            request_id: Some("live-pulse".into()),
        },
    )
    .await;
    act(&mut session, rule("pulse", "live-work")).await;
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if store::records::get(&cell.engine().store.pool, &records[2])
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_string()
                == "-1"
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the normal Cell scheduler must execute Karma without simulated stepping");
    assert!(nucleus::execution::current().is_none());
    cell.shutdown().await;
}
