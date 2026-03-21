use crate::{
    auth::AuthSubject,
    view::{ViewReadService, ViewSnapshotEnvelope},
};
use persistence::write_coordinator::WriteCoordinatorHandle;
use serde_json::json;
use std::{collections::BTreeSet, io::Error};
use tokio::sync::mpsc;

pub struct SubscriptionHandle {
    pub rx: mpsc::UnboundedReceiver<SseFrame>,
}

#[derive(Debug, Clone)]
pub enum SseFrame {
    Snapshot { payload: String },
    Error { payload: String },
}

#[derive(Clone)]
pub struct SubscriptionRegistry {
    view_reads: ViewReadService,
    writer: WriteCoordinatorHandle,
}

impl SubscriptionRegistry {
    pub fn new(view_reads: ViewReadService, writer: WriteCoordinatorHandle) -> Self {
        Self { view_reads, writer }
    }

    pub async fn subscribe_view(
        &self,
        _actor: AuthSubject,
        view_id: u32,
    ) -> Result<SubscriptionHandle, Error> {
        let initial = self.view_reads.read_snapshot(view_id).await?;
        let dependencies = self.view_reads.dependencies(view_id).await?;
        let mut invalidation_rx = self.writer.subscribe_invalidations();
        let view_reads = self.view_reads.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let initial_payload = initial.payload.clone();

        tx.send(SseFrame::Snapshot {
            payload: initial_payload.clone(),
        })
        .map_err(|_| Error::other("Subscription closed before initial snapshot"))?;

        tokio::spawn(async move {
            let mut dependencies = dependencies;
            let mut last_payload = initial_payload;

            loop {
                let should_refresh = match invalidation_rx.recv().await {
                    Ok(event) => event
                        .changed_tables
                        .iter()
                        .any(|table| dependencies.contains(table)),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                if !should_refresh {
                    continue;
                }

                match refresh_subscription(&view_reads, view_id, &mut dependencies, &last_payload)
                    .await
                {
                    Ok(Some(envelope)) => {
                        last_payload = envelope.payload.clone();
                        if tx
                            .send(SseFrame::Snapshot {
                                payload: envelope.payload,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let payload = serialize_error(&error);
                        if tx.send(SseFrame::Error { payload }).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(SubscriptionHandle { rx })
    }
}

async fn refresh_subscription(
    view_reads: &ViewReadService,
    view_id: u32,
    dependencies: &mut BTreeSet<String>,
    last_payload: &str,
) -> Result<Option<ViewSnapshotEnvelope>, Error> {
    *dependencies = view_reads.dependencies(view_id).await?;
    let envelope = view_reads.read_snapshot(view_id).await?;
    if envelope.payload == last_payload {
        Ok(None)
    } else {
        Ok(Some(envelope))
    }
}

fn serialize_error(error: &Error) -> String {
    serde_json::to_string(&json!({ "error": error.to_string() }))
        .unwrap_or_else(|_| "{\"error\":\"failed to serialize error\"}".to_string())
}
