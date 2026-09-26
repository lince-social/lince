use super::{ALPN_THREAD, EndpointAddr, Wire, WireRequest, WireResponse};
use crate::EngineError;

impl Wire {
    pub(super) async fn sync_groups(&self) -> Result<(), EngineError> {
        let Some(local) = store::organs::local(&self.engine.store.pool).await? else {
            return Ok(());
        };
        for signed in self.engine.groups().await? {
            let group = &signed.membership;
            let accepted = self.engine.group_accepted_locally(&group.root).await?;
            self.engine.save_group(&signed, accepted).await?;
            if accepted {
                self.engine.apply_group_grants(group).await?;
            }
            if group.owner == local.uid {
                for member in group
                    .members
                    .iter()
                    .filter(|member| member.organ != local.uid)
                {
                    let delivered: Option<i64> = store::sqlx::query_scalar(
                        "SELECT revision FROM conversation_delivery WHERE root = ? AND organ = ?",
                    )
                    .bind(&group.root)
                    .bind(&member.organ)
                    .fetch_optional(&self.engine.store.pool)
                    .await?;
                    if delivered.is_some_and(|revision| revision >= group.revision) {
                        continue;
                    }
                    if store::organs::contact(&self.engine.store.pool, &member.organ)
                        .await?
                        .is_none_or(|contact| contact.trust == "blocked")
                    {
                        continue;
                    }
                    let Ok(id) = member.node_id.parse() else {
                        continue;
                    };
                    let response = tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        self.request(
                            EndpointAddr::new(id),
                            ALPN_THREAD,
                            &WireRequest::GroupOffer {
                                group: signed.clone(),
                            },
                        ),
                    )
                    .await;
                    if matches!(response, Ok(Ok(WireResponse::Applied { .. }))) {
                        store::sqlx::query("INSERT INTO conversation_delivery(root, organ, revision) VALUES (?, ?, ?) ON CONFLICT(root, organ) DO UPDATE SET revision = excluded.revision")
                            .bind(&group.root).bind(&member.organ).bind(group.revision).execute(&self.engine.store.pool).await?;
                    }
                }
            } else if self.engine.group_accepted_locally(&group.root).await? {
                let Some(own) = group
                    .members
                    .iter()
                    .find(|member| member.organ == local.uid)
                else {
                    continue;
                };
                if own.removed || own.accepted {
                    continue;
                }
                let Some(owner) = group
                    .members
                    .iter()
                    .find(|member| member.organ == group.owner)
                else {
                    continue;
                };
                let Ok(id) = owner.node_id.parse() else {
                    continue;
                };
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    self.request(
                        EndpointAddr::new(id),
                        ALPN_THREAD,
                        &WireRequest::GroupAccept {
                            root: group.root.clone(),
                        },
                    ),
                )
                .await;
                if let Ok(Ok(WireResponse::Group { group: current })) = response {
                    self.engine.receive_group(&group.owner, &current).await?;
                }
            }
        }
        Ok(())
    }
}
