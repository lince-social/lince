use super::fixtures::{engine, protein};
use protein::{Predicate, Source, execute};

#[tokio::test]
async fn concept_source_reads_the_lingua_dag() {
    let e = engine().await;
    let initial = execute(&e.store, &protein(Source::Concept)).await.unwrap();
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let fruit = store::concepts::create(&e.store.pool, "fruit", &[&food])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "apple", &[&fruit])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "tool", &[])
        .await
        .unwrap();

    let rows = execute(&e.store, &protein(Source::Concept)).await.unwrap();
    assert_eq!(rows.len(), initial.len() + 4);
    assert!(
        initial
            .iter()
            .all(|before| rows.iter().any(|row| row["uid"] == before["uid"]))
    );
    let apple = rows.iter().find(|r| r["name"] == "apple").unwrap();
    assert_eq!(apple["parents"][0], fruit.as_str());

    let mut q = protein(Source::Concept);
    q.filter = vec![Predicate::ConceptIn("food".into())];
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| {
        let name = row["name"].as_str().expect("concept name");
        name != "tool"
    }));
}
