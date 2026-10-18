use super::*;

async fn fiote(host: &Host, name: &str, parent: Option<&str>) -> String {
    let uid = host
        .engine
        .act(
            Action::CreateAgent {
                head: name.into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.handle(Request::Behavior {
        record: uid.clone(),
        prompt_parent: parent.map(str::to_owned),
        run_assigned: false,
    })
    .await
    .unwrap();
    uid
}

#[tokio::test]
async fn intake_ranks_direct_children_then_creation_instant_and_identity() {
    let (host, _, _root, _, _) = fixture(false).await;
    let first = fiote(&host, "First intake", None).await;
    let second = fiote(&host, "Second intake", None).await;
    for parent in [&first, &second] {
        for index in 0..3 {
            fiote(&host, &format!("Child {index}"), Some(parent)).await;
        }
    }
    store::sqlx::query("UPDATE record SET created_at = ? WHERE uid = ?")
        .bind("2020-01-01T01:00:00+01:00")
        .bind(&first)
        .execute(&host.engine.store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET created_at = ? WHERE uid = ?")
        .bind("2020-01-01T00:00:01Z")
        .bind(&second)
        .execute(&host.engine.store.pool)
        .await
        .unwrap();
    let selected = host.intake_selection().await.unwrap().unwrap();
    assert_eq!(selected.record, first);
    assert_eq!(selected.children, 3);
    assert!(!selected.enabled);
    assert_eq!(host.handle(Request::Directory).await.unwrap().record, first);
    store::sqlx::query("UPDATE record SET created_at = ? WHERE uid = ?")
        .bind("2020-01-01T00:00:00Z")
        .bind(&second)
        .execute(&host.engine.store.pool)
        .await
        .unwrap();
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        *[first.clone(), second.clone()].iter().min().unwrap()
    );
    let additional = fiote(&host, "Additional child", Some(&second)).await;
    let grandchild = fiote(&host, "Grandchild", Some(&additional)).await;
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        second
    );
    assert_eq!(host.intake_selection().await.unwrap().unwrap().children, 4);
    host.engine
        .act(Action::DeleteRecord { target: grandchild }, None)
        .await
        .unwrap();
    host.engine
        .act(Action::DeleteRecord { target: additional }, None)
        .await
        .unwrap();
    assert_eq!(host.intake_selection().await.unwrap().unwrap().children, 3);
}

#[tokio::test]
async fn intake_reparenting_changes_new_selection_without_changing_existing_threads() {
    let (host, _, _root, record, thread) = fixture(false).await;
    let original = host.thread_fiote(&thread).await.unwrap();
    let other = fiote(&host, "New hierarchy", None).await;
    for index in 0..3 {
        fiote(&host, &format!("New child {index}"), Some(&other)).await;
    }
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        other
    );
    assert_eq!(host.thread_fiote(&thread).await.unwrap(), original);
    assert_eq!(
        host.status(&record).await.unwrap().intake.unwrap().record,
        other
    );
    let opened = Host::open(host.engine.clone(), host.directory.clone())
        .await
        .unwrap();
    assert_eq!(
        opened.intake_selection().await.unwrap().unwrap().record,
        other
    );
}

#[tokio::test]
async fn intake_uses_zero_child_candidates_and_reports_an_empty_hierarchy() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let root = tempfile::tempdir().unwrap();
    let host = Host::open(engine, root.path().join("fiote")).await.unwrap();
    assert!(host.intake_selection().await.unwrap().is_none());
    let uid = host
        .engine
        .act(
            Action::CreateAgent {
                head: "Only Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    host.engine
        .act(
            Action::ConfigureFiote {
                target: uid.clone(),
                prompt_parent: None,
                run_assigned: false,
            },
            None,
        )
        .await
        .unwrap();
    let selected = host.intake_selection().await.unwrap().unwrap();
    assert_eq!(selected.record, uid);
    assert_eq!(selected.children, 0);
    host.engine
        .act(Action::DeleteRecord { target: uid }, None)
        .await
        .unwrap();
    assert!(host.intake_selection().await.unwrap().is_none());
}

#[tokio::test]
async fn intake_selection_cannot_be_requested_from_a_remote_session() {
    let (host, _, _root, _, _) = fixture(false).await;
    let mut session = Session::new(
        host.engine.clone(),
        Arc::new(LaneHub::new()),
        "remote-intake",
        None,
    )
    .with_fiote(host);
    assert!(matches!(
        session
            .handle(ClientMessage::Fiote {
                id: "intake".into(),
                request: Request::Directory,
            })
            .await
            .first(),
        Some(ServerMessage::Error { .. })
    ));
}

#[tokio::test]
async fn intake_and_discovery_include_candidates_beyond_one_thousand_names() {
    let (host, _, _root, record, _) = fixture(false).await;
    let selected = fiote(&host, "ZZ selected intake", None).await;
    for index in 0..3 {
        fiote(&host, &format!("ZZ child {index}"), Some(&selected)).await;
    }
    let organ = host.record(&record).await.unwrap().organ_uid.unwrap();
    let mut tx = host.engine.store.pool.begin().await.unwrap();
    for index in 0..1001 {
        let uid = nucleus::new_uid("r");
        store::records::create_with_uid_on(
            &mut tx,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: &format!("AA filler {index}"),
                body: "",
                quantity: store::exact::zero(),
            },
            &organ,
            None,
        )
        .await
        .unwrap();
        store::records::set_extension_on(
            &mut tx,
            &uid,
            "lince.fiote",
            &json!({"run_assigned":false}),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let choices = host.fiote_choices().await.unwrap();
    assert!(choices.len() > 1000);
    assert!(choices.iter().any(|choice| choice.record == selected));
    assert_eq!(
        host.intake_selection().await.unwrap().unwrap().record,
        selected
    );
}
