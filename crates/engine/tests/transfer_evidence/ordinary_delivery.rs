use super::published_events::action;
use super::*;
use nucleus::transfer::{
    receipt::{Command, Document},
    receipt_delivery::{Command as DeliveryCommand, Delivery, Request, Source},
};

pub(super) async fn receive(
    sender: &Engine,
    author: &Person,
    source: Source,
    receiver: &Engine,
    reader: &Person,
    id: &str,
) -> (String, Delivery) {
    sender.set_signer(author.signer.clone()).await.unwrap();
    receiver.set_signer(reader.signer.clone()).await.unwrap();
    let recipient = sender
        .adopt_introduction(&receiver.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    let origin = receiver
        .adopt_introduction(&sender.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    for (node, contact) in [(sender, &recipient), (receiver, &origin)] {
        store::organs::set_trust(&node.store.pool, contact, "known")
            .await
            .unwrap();
        store::organs::set_sync_policy(&node.store.pool, contact, false, false)
            .await
            .unwrap();
    }
    let (root, thread) = sender.start_conversation(&recipient, id).await.unwrap();
    store::replica::accept(&sender.store.pool, &root, &recipient)
        .await
        .unwrap();
    store::replica::accept(&receiver.store.pool, &root, &origin)
        .await
        .unwrap();
    for target in [&root, &thread] {
        store::visibility::grant(&sender.store.pool, "actor", Some(&author.uid), target)
            .await
            .unwrap();
    }
    let mut expires_at = nucleus::execution::now().timestamp() + 3600;
    if let Source::Document { record, .. } = &source {
        expires_at = expires_at.min(
            store::transfer_receipts::get(&sender.store.pool, record)
                .await
                .unwrap()
                .document
                .expires_at(),
        );
    }
    let request = Request {
        source: source.clone(),
        thread,
        recipients: vec![recipient.clone()],
        expires_at,
    };
    let preview = sender
        .act(
            action(
                author,
                &format!("{id}:preview"),
                Command::Delivery {
                    request: DeliveryCommand::Preview {
                        request: request.clone(),
                    },
                },
            ),
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["receipt"]["document"]["value"]
        .clone();
    let sent = sender
        .act(
            action(
                author,
                &format!("{id}:send"),
                Command::Delivery {
                    request: DeliveryCommand::Send {
                        request,
                        reviewed_at: preview["reviewed_at"].as_i64().unwrap(),
                        expected_projection: serde_json::from_value(
                            preview["expected_projection"].clone(),
                        )
                        .unwrap(),
                    },
                },
            ),
            None,
        )
        .await
        .unwrap();
    let message = sent.created.unwrap();
    let page = sender
        .export_conversation_page(&recipient, &root, &[], 2000)
        .await
        .unwrap();
    assert!(page.batch.ops.iter().any(|op| op.uid == message));
    let wire = serde_json::to_string(&page.batch).unwrap();
    assert!(!wire.contains(&author.uid));
    if let Source::Document { record, .. } = &source {
        assert!(!wire.contains(record));
    }
    let baseline: (i64, i64, i64) = store::sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM transfer_receipt_version),(SELECT COUNT(*) FROM transfer),(SELECT COUNT(*) FROM transfer_occurrence)",
    )
    .fetch_one(&receiver.store.pool)
    .await
    .unwrap();
    receiver
        .import_grant_batch(&root, &page.batch)
        .await
        .unwrap();
    let unchanged: (i64, i64, i64) = store::sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM transfer_receipt_version),(SELECT COUNT(*) FROM transfer),(SELECT COUNT(*) FROM transfer_occurrence)",
    )
    .fetch_one(&receiver.store.pool)
    .await
    .unwrap();
    assert_eq!(unchanged, baseline);
    assert!(
        store::records::get(&receiver.store.pool, &sent.facts[0].record_uid)
            .await
            .unwrap()
            .is_none()
    );
    let inspected = receiver
        .act(
            action(
                reader,
                &format!("{id}:inspect"),
                Command::Delivery {
                    request: DeliveryCommand::Inspect {
                        message: message.clone(),
                    },
                },
            ),
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["receipt"]["document"]["value"]
        .clone();
    (
        message,
        serde_json::from_value(inspected["artifact"].clone()).unwrap(),
    )
}

pub(super) async fn exchange(
    sender: &Engine,
    author: &Person,
    record: &str,
    receiver: &Engine,
    reader: &Person,
    id: &str,
) -> String {
    let saved = store::transfer_receipts::get(&sender.store.pool, record)
        .await
        .unwrap();
    let (message, delivery) = receive(
        sender,
        author,
        Source::Document {
            record: record.into(),
            expected_version: saved.version,
        },
        receiver,
        reader,
        id,
    )
    .await;
    let imported = receiver
        .act(
            action(
                reader,
                &format!("{id}:import"),
                Command::Delivery {
                    request: DeliveryCommand::Import {
                        message,
                        expected_projection: delivery.hash().unwrap(),
                    },
                },
            ),
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let received: Document = store::transfer_receipts::get(&receiver.store.pool, &imported)
        .await
        .unwrap()
        .document;
    assert_eq!(received, saved.document);
    imported
}
