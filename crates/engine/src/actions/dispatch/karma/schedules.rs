// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_schedules(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SaveKarmaSchedule {
                schedule,
                expected_revision,
                name,
                boundaries,
                request_id,
            } => {
                outcome = Box::pin(self.save_karma_schedule(
                    schedule,
                    expected_revision,
                    name,
                    boundaries,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
                self.notify_karma_deadline_change();
            }
            Action::InspectKarmaSchedules { schedule } => {
                outcome.data = Some(
                    self.inspect_karma_schedules(schedule.as_deref(), actor.as_deref())
                        .await?,
                );
            }
            Action::PreviewKarmaScheduleDates {
                date,
                timezone,
                gap,
                fold,
            } => {
                outcome.data = Some(
                    self.preview_schedule_dates(date, timezone, gap, fold, actor.as_deref())
                        .await?,
                );
            }
            Action::CancelKarmaSchedule {
                schedule,
                expected_revision,
                request_id,
            } => {
                outcome = Box::pin(self.cancel_karma_schedule(
                    schedule,
                    expected_revision,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
                self.notify_karma_deadline_change();
            }
            Action::RetryKarmaSchedule {
                schedule,
                expected_revision,
                boundary,
                request_id,
            } => {
                outcome = Box::pin(self.retry_karma_schedule(
                    schedule,
                    expected_revision,
                    boundary,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
                self.notify_karma_deadline_change();
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
