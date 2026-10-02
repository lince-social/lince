use super::*;

async fn record(engine: &Engine, kind: nucleus::RecordKind) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Private health fixture",
            body: "Never expose this private body",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn mixed_delivery_health_distinguishes_messages_copies_attempts_and_unknown_metadata_after_reopen()
 {
    let clock = nucleus::execution::Execution::new([242; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let engine = Engine::open_memory().await.unwrap();
        let now = clock.now().timestamp();
        let context = record(&engine, nucleus::RecordKind::Plain).await;
        let mut messages = Vec::new();
        for (issued, expiry, retry, error) in [(json!(now-120),now+100,0,None),(json!(now+10),now+100,now+30,Some("Secret raw retry error")),(json!("unknown"),now-1,0,Some("Secret expired error"))] {
            let message = record(&engine, nucleus::RecordKind::Message).await;
            store::records::set_extension(&engine.store.pool, &message, "lince.social.message", &json!({"content":{"issued_at":issued,"kind":{"text":"Never expose this Message"}}})).await.unwrap();
            store::sqlx::query("INSERT INTO social_message_work(record_uid,conversation,context,expires_at,next_attempt,error) VALUES(?,?,?,?,?,?)")
                .bind(&message).bind(&context).bind(&context).bind(expiry).bind(retry).bind(error).execute(&engine.store.pool).await.unwrap();
            messages.push(message);
        }
        let mut copies = Vec::new();
        for (index,state,body,expiry,message) in [(0,"pending",json!({"envelope":{"created_at":now-120}}).to_string(),now+100,&messages[0]),(1,"stored",json!({"envelope":{"created_at":now-120}}).to_string(),now+100,&messages[0]),(2,"held",json!({"envelope":{"created_at":now+10}}).to_string(),now+100,&messages[1]),(3,"expired","{".into(),now-1,&messages[2]),(4,"ready",json!({"envelope":{"created_at":"unknown"}}).to_string(),now+100,&messages[2]),(5,"cancelled",json!({"envelope":{"created_at":now-20}}).to_string(),now+100,&messages[2])] {
            let id = format!("Private envelope {index}");
            store::sqlx::query("INSERT INTO social_private_outbox(id,context,body,hash,expires_at,state,record_uid) VALUES(?,?,?,'private hash',?,?,?)")
                .bind(&id).bind(&context).bind(body).bind(expiry).bind(state).bind(message).execute(&engine.store.pool).await.unwrap();
            copies.push(id);
        }
        for (copy,service,state,retry,receipt,error) in [(0,"Private endpoint A","pending",0,None,None),(0,"Private endpoint B","stored",now+45,None,Some("Secret raw retry error")),(1,"Private endpoint A","stored",now+15,Some("{\"stage\":\"stored\"}"),None),(2,"Private endpoint A","pending",0,None,None),(3,"Private endpoint A","pending",0,Some("{"),Some("Secret receipt error")),(4,"Private endpoint A","stored",0,Some("{\"stage\":\"recipient-durable\"}"),None),(5,"Private endpoint A","failed",0,Some("{\"stage\":\"recipient-refused\"}"),Some("Secret refusal error"))] {
            store::sqlx::query("INSERT INTO social_private_destination(envelope,service,state,next_attempt,receipt,error) VALUES(?,?,?,?,?,?)")
                .bind(&copies[copy]).bind(service).bind(state).bind(retry).bind(receipt).bind(error).execute(&engine.store.pool).await.unwrap();
        }
        store::sqlx::query("INSERT INTO social_publication_job(hash,destination,kind,body,expires_at) VALUES('private publication','private host','snippet','{',?)").bind(now+100).execute(&engine.store.pool).await.unwrap();
        store::sqlx::query("INSERT INTO social_pickup_work(context,service,next_attempt,error) VALUES(?,'Private endpoint A',?,'Secret pickup error')").bind(&context).bind(now+20).execute(&engine.store.pool).await.unwrap();
        for (envelope,discard,expiry,retry) in [("Deferred private ciphertext",0,now+100,0),("Discard private ciphertext",1,now+100,now+60),("Elapsed private ciphertext",1,now-1,0)] {
            store::sqlx::query("INSERT INTO social_receive_failure(context,service,envelope,reference,error,expires_at,discard,next_attempt) VALUES(?,'Private endpoint A',?,'{}','Secret receive error',?,?,?)")
                .bind(&context).bind(envelope).bind(expiry).bind(discard).bind(retry).execute(&engine.store.pool).await.unwrap();
        }
        store::sqlx::query("INSERT INTO social_reply_route(id,owner,signing_key,pickup_key,body,created_at,expires_at) VALUES('private route','private owner','private signing key','private pickup key','{}',?,?)")
            .bind(now).bind(now+100).execute(&engine.store.pool).await.unwrap();
        for (index,state,expiry) in [(0,"accepted",now+100),(1,"provisional",now+100),(2,"blocked",now-1),(3,"closed",now-1)] {
            store::sqlx::query("INSERT INTO social_sender_admission(route,sender,state,body,issued_at,expires_at) VALUES('private route',?,?,'{}',?,?)")
                .bind(format!("Private sender {index}")).bind(state).bind(now).bind(expiry).execute(&engine.store.pool).await.unwrap();
        }
        for (context,state) in [(&context,"active"),(&messages[0],"dormant"),(&messages[1],"retired"),(&messages[2],"review")] {
            store::sqlx::query("INSERT INTO social_context_retention(context,state,retire_after,checked_at,error) VALUES(?,?,?,?,?)")
                .bind(context).bind(state).bind(now+200).bind(now).bind("Secret retention error").execute(&engine.store.pool).await.unwrap();
        }
        let organ = store::organs::local(&engine.store.pool).await.unwrap().unwrap().uid;
        store::records::set_extension(&engine.store.pool, &context, "lince.social.blocks", &json!({"Private peer A":{"blocked":true,"window":now-10},"Private peer B":{"blocked":true,"window":now}})).await.unwrap();
        store::records::set_extension(&engine.store.pool, &organ, "lince.social.retained-blocks", &json!({"Private override":{"context":context,"peer":"Private peer A","blocked":false,"window":now}})).await.unwrap();
        let health = command(&engine, Command::ServiceHealth).await;
        let storage = &health["service_health"]["storage"];
        assert_eq!(storage["mailbox_reserved_entries"], 5);
        assert_eq!(storage["mailbox_reserved_budget_bytes"], 3082);
        assert_eq!(storage["mailbox_entry_limit"], 10000);
        assert_eq!(storage["mailbox_data_entry_limit"], 7500);
        assert_eq!(storage["mailbox_byte_limit"], 512 * 1024 * 1024);
        assert_eq!(storage["mailbox_data_byte_limit"], 384 * 1024 * 1024);
        let delivery = &health["service_health"]["delivery"];
        assert_eq!(delivery["preparation"], json!({"messages":3,"due":1,"delayed":1,"elapsed":1,"errors":2,"unknown_age":1,"future_age":1,"oldest_age_seconds":120,"next_retry_seconds":0}));
        assert_eq!(delivery["copies"]["copies"],6);
        assert_eq!(delivery["copies"]["message_records"],3);
        for state in ["pending","stored","held","expired","ready","cancelled"] { assert_eq!(delivery["copies"][state],1); }
        assert_eq!(delivery["copies"]["unknown_age"],2);
        assert_eq!(delivery["copies"]["future_age"],1);
        assert_eq!(delivery["copies"]["oldest_age_seconds"],120);
        assert_eq!(delivery["destinations"]["attempts"],7);
        assert_eq!(delivery["destinations"]["due"],1);
        assert_eq!(delivery["destinations"]["delayed"],2);
        assert_eq!(delivery["destinations"]["recipient_durable"],1);
        assert_eq!(delivery["destinations"]["recipient_refused"],1);
        assert_eq!(delivery["destinations"]["unknown_receipts"],1);
        assert_eq!(delivery["pickup"]["next_retry_seconds"],20);
        assert_eq!(delivery["receive_failures"],json!({"retained":3,"deferred":1,"discard_pending":1,"elapsed":1,"due":0,"delayed":1,"next_retry_seconds":60}));
        assert_eq!(delivery["host_admissions"],json!({"retained":4,"current":2,"elapsed":2,"accepted":1,"provisional":1,"blocked_evidence":1,"closed_evidence":1}));
        assert_eq!(delivery["blocks"]["active_blocks"],1);
        assert_eq!(delivery["blocks"]["unblocks"],1);
        assert_eq!(delivery["key_retention"]["deleted_block_overrides"],1);
        assert_eq!(delivery["key_retention"]["next_retirement_at"],now+200);
        assert!(health["service_health"]["queues"]["publication_oldest_age_seconds"].is_null());
        let encoded = health.to_string();
        assert!(encoded.len()<8192);
        for private in messages.iter().chain([context,organ].iter()) { assert!(!encoded.contains(private)); }
        for private in ["Private ","Never expose","Secret ","private owner","private signing key","private hash"] { assert!(!encoded.contains(private)); }
        let reopened = Engine::new(engine.store.clone()).await.unwrap();
        assert_eq!(command(&reopened, Command::ServiceHealth).await["service_health"]["delivery"],*delivery);
    })).await;
}
