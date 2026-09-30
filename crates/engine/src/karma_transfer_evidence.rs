use chrono::{DateTime, Utc};
use nucleus::transfer::karma::Snapshot;

use crate::{Engine, EngineError};

impl Engine {
    pub async fn observe_karma_transfer_projection(
        &self,
        transfer: &str,
        envelope: &str,
        parent: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        self.access_scope(true, async {
            if let Some(parent) = parent
                && let Some(execution) = nucleus::execution::current()
                && let Some(control) = execution.control()
                && let Some(cell) = execution.cell()
            {
                control.link_received(cell, envelope, parent);
            }
            Box::pin(self.react_to_event(vec![transfer.into()], envelope.into(), now)).await?;
            self.notify_query_changed();
            Ok(())
        })
        .await
    }

    pub async fn read_karma_transfer_state(
        &self,
        transfer: &str,
        actor: Option<&str>,
    ) -> Result<Snapshot, EngineError> {
        self.access_scope(false, self.transfer_karma_snapshot(transfer, actor))
            .await
    }

    pub async fn check_karma_input_visibility(
        &self,
        actor: Option<&str>,
        inputs: &[String],
    ) -> Result<(), EngineError> {
        self.access_scope(false, self.refuse_unreadable_karma_inputs(actor, inputs))
            .await
    }

    pub async fn clone_karma_preview_signer(
        &self,
        actor: Option<&str>,
    ) -> Result<Option<crate::trust::Signer>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let Some(signer) = signer else {
            return Ok(None);
        };
        if store::records::get(&self.store.pool, &signer.actor_uid)
            .await?
            .is_none_or(|record| record.kind != "person")
        {
            return Ok(None);
        }
        if actor.is_some() && self.actor_person(actor).await?.as_deref() != Some(&signer.actor_uid)
        {
            return Ok(None);
        }
        Ok(Some(signer))
    }

    pub(crate) fn observe_karma_transfer_change(
        &self,
        before: Snapshot,
        after: Snapshot,
        now: DateTime<Utc>,
        command: Option<&str>,
    ) -> Result<(), EngineError> {
        if before == after {
            return Ok(());
        }
        if let Some(execution) = nucleus::execution::current()
            && let Some(control) = execution.control()
            && let Some(cell) = execution.cell()
        {
            let change = nucleus::simulation::TransferChange {
                cell: cell.into(),
                at_ms: now.timestamp_millis(),
                virtual_ms: control.now_ms(),
                before,
                after,
            };
            if let Some(command) = command {
                control
                    .record_received_transfer_change(command, change)
                    .map_err(EngineError::ExecutionLimit)?;
            } else if let Ok(occurrence) =
                crate::rule_runtime::EFFECT_OCCURRENCE.try_with(Clone::clone)
            {
                control
                    .record_transfer_change(cell, &occurrence, change)
                    .map_err(EngineError::ExecutionLimit)?;
            }
        }
        Ok(())
    }
}
