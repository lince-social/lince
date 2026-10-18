use super::*;
use futures::StreamExt;

fn selected(source: &SourceLease, expected: &[SourceLease]) -> bool {
    source.settings.source_kind == SourceKind::Device
        && expected.iter().any(|previous| {
            previous.settings.record_uid == source.settings.record_uid
                && previous.session_uid == source.session_uid
        })
}

impl Engine {
    pub async fn publish_device_fix_for(
        &self,
        expected: &[SourceLease],
        fix: Fix,
    ) -> Result<(), EngineError> {
        self.location_expire().await?;
        fix.validate().map_err(invalid)?;
        for source in self
            .location_sources()
            .await
            .into_iter()
            .filter(|source| selected(source, expected))
        {
            self.location_publish_source(&source, fix.clone()).await?;
        }
        Ok(())
    }

    pub async fn location_device_unavailable_for(&self, expected: &[SourceLease]) {
        let sources = self
            .location_sources()
            .await
            .into_iter()
            .filter(|source| selected(source, expected))
            .collect::<Vec<_>>();
        {
            let mut state = self.location.state.lock().await;
            for source in &sources {
                if state
                    .sources
                    .get(&source.settings.record_uid)
                    .is_some_and(|active| active.lease.session_uid == source.session_uid)
                {
                    state.sources.remove(&source.settings.record_uid);
                }
            }
        }
        futures::stream::iter(sources)
            .for_each_concurrent(4, |source| async move {
                if self
                    .location_node_id()
                    .is_ok_and(|node| node == source.authority_node_id)
                {
                    self.location_stop_record_if(
                        &source.settings.record_uid,
                        Some(&source.session_uid),
                    )
                    .await;
                } else if let Ok(network) = self.location_network() {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(5),
                        network.location_request(
                            &source.authority_node_id,
                            PeerRequest::Unavailable {
                                record_uid: source.settings.record_uid,
                                session_uid: source.session_uid,
                            },
                        ),
                    )
                    .await;
                }
            })
            .await;
    }
}
