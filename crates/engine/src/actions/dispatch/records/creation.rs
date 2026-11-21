// non-lean-yet

use super::super::{Action, ControlFlow, DateTime, DispatchOutcome, Engine, Utc, invalid_dispatch};

impl Engine {
    pub(in super::super) async fn dispatch_records_creation(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let outcome;
        match action {
            Action::CreateRecordWithTags {
                head,
                body,
                quantity,
                tags,
            } => {
                outcome = self
                    .create_tagged_record(head, body, quantity, tags, actor, now)
                    .await?;
            }
            Action::CreateRecordDraft { draft } => {
                outcome = self.create_record_draft(draft, actor, now).await?;
            }
            Action::CreateRecord {
                slug,
                kind,
                head,
                body,
                quantity,
            } => {
                outcome = self
                    .create_record_draft_kind(
                        crate::record_creation::Draft {
                            head,
                            body,
                            slug,
                            quantity: store::exact::from_f64(quantity).to_string(),
                            ..Default::default()
                        },
                        kind,
                        actor,
                        now,
                    )
                    .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
