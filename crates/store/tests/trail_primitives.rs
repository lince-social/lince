use nucleus::{RecordKind,knowledge::Item};
use serde_json::json;

async fn record(store:&store::Store,slug:&str) -> String {
    store::records::create(&store.pool,store::records::NewRecord{slug:Some(slug),kind:RecordKind::Plain,head:slug,body:"",quantity:store::exact::zero()}).await.unwrap().uid
}

#[tokio::test]
async fn assertion_conditions_keep_same_named_record_and_concept_distinct() {
    let store=store::Store::open_memory().await.unwrap();
    let response=record(&store,"response").await;
    let answer=record(&store,"answer").await;
    let predicate=nucleus::new_uid("c");
    store::sqlx::query("INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,'answer','2026-01-01T00:00:00Z')").bind(&predicate).execute(&store.pool).await.unwrap();
    let source="has_assertion(@response, #answer, @answer)";
    let bindings=store::karma_bindings::resolve(&store.pool,source,&[]).await.unwrap();
    assert_eq!(bindings.len(),3);
    let bound=store::karma_bindings::apply(source,&bindings).unwrap();
    assert_eq!(bound.reads()[0].slug,format!("{response}|{predicate}|{answer}"));
}

#[tokio::test]
async fn saved_collections_and_reviews_can_exceed_sixteen_mib() {
    let store=store::Store::open_memory().await.unwrap();
    let uid=record(&store,"large-collection").await;
    let item=Item{source:"test".into(),id:"collection".into(),revision:"1".into(),title:"Collection".into(),content:"A collection of referenced items".into(),content_type:"text/plain".into(),uri:"https://example.invalid/collection".into(),license:None,credits:vec![],meanings:vec![],claims:vec![],dependencies:vec![],assets:vec![],raw:String::new()};
    let payload=json!({"collection":"x".repeat(17*1024*1024)});
    let mut transaction=store::write_tx(&store.pool).await.unwrap();
    store::knowledge::save_payload_on(&mut transaction,&uid,&item,&payload).await.unwrap();
    assert_eq!(store::knowledge::payload_on(&mut transaction,&uid).await.unwrap().unwrap(),payload);
    store::sqlx::query("INSERT INTO knowledge_review(uid,owner_key,kind,content,created_at) VALUES('large-review','local','save',?,'2026-01-01T00:00:00Z')").bind(payload.to_string()).execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}
