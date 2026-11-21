use super::fixtures::{engine, make};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::execute_saved;
use serde_json::json;

#[tokio::test]
async fn saved_protein_is_a_record() {
    let e = engine().await;
    make(&e, "a", RecordKind::Plain, -1.0).await;
    make(&e, "b", RecordKind::Plain, 5.0).await;

    let ast = json!({
        "source": "record",
        "where": [{ "all": [{ "quantity_lt": "0.0" }] }]
    });
    e.act(
        Action::SaveProtein {
            slug: "views.my-needs".into(),
            head: "My Needs".into(),
            ast,
        },
        None,
    )
    .await
    .unwrap();

    let rows = execute_saved(&e.store, "views.my-needs", None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "a");
}
