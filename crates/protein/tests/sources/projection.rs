use super::fixtures::{engine, plain, protein};
use engine::actions::Action;
use nucleus::PromiseState;
use protein::{
    ExtensionInclude, Include, LinkDirection, LinksInclude, Order, Predicate, ProjectionInclude,
    Source, execute,
};
use serde_json::json;

#[tokio::test]
async fn extension_and_projection_includes_attach() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 5.0).await;
    e.act(
        Action::SetExtension {
            target: apples.clone(),
            namespace: "task.effort".into(),
            fds: json!({ "points": 3 }),
        },
        None,
    )
    .await
    .unwrap();
    store::misc::insert_promise(
        &e.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 3.0,
            window_end: Some("2026-08-01T00:00:00Z".into()),
            state: Some(PromiseState::Agreed),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::UidEq(apples.clone())];
    q.include = Include {
        extension: Some(ExtensionInclude {
            namespace: "task.effort".into(),
        }),
        projection: Some(ProjectionInclude {
            at: "2026-09-01T00:00:00Z".into(),
        }),
        ..Default::default()
    };
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["extension"]["points"], 3);
    assert!(rows[0]["projected"]["quantity"].is_null());
    assert_eq!(rows[0]["projected"]["status"], "scenario_required");

    q.include.projection = Some(ProjectionInclude {
        at: "2026-07-15T00:00:00Z".into(),
    });
    let rows = execute(&e.store, &q).await.unwrap();
    assert!(rows[0]["projected"]["quantity"].is_null());
    assert_eq!(rows[0]["projected"]["at"], "2026-07-15T00:00:00Z");
}

#[tokio::test]
async fn link_depth_expands_the_tree() {
    let e = engine().await;
    plain(&e, "a", 0.0).await;
    plain(&e, "b", 0.0).await;
    plain(&e, "c", 0.0).await;
    store::concepts::create(&e.store.pool, "needs", &[])
        .await
        .unwrap();
    for (from, to) in [("a", "b"), ("b", "c")] {
        e.act(
            Action::AssertRecord {
                subject: from.into(),
                predicate: "needs".into(),
                object: Some(to.into()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut q = protein(Source::Record);
    q.filter = vec![Predicate::SlugEq("a".into())];
    q.include = Include {
        links: Some(LinksInclude {
            kinds: vec!["needs".into()],
            direction: LinkDirection::Out,
            depth: 0,
        }),
        ..Default::default()
    };
    q.order = vec![Order::Asc("slug".into())];
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(rows[0]["links"].as_array().unwrap().len(), 1);

    q.include.links.as_mut().unwrap().depth = 2;
    let rows = execute(&e.store, &q).await.unwrap();
    let links = rows[0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 2, "hop 2 reaches b -> c");
    assert!(links.iter().any(|l| l["hop"] == 1));
    assert!(links.iter().any(|l| l["hop"] == 2));
}
