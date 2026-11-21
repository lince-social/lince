// non-lean-yet

use super::*;

impl Document {
    pub(super) fn dispatch(
        &mut self,
        needs_text: bool,
        draft: Option<&engine::record_edit::Write>,
    ) -> Vec<tokio::sync::oneshot::Sender<bool>> {
        let _admission = performance::Span::new(Stage::RecordEditAdmission);
        let mut gates = Vec::new();
        if (!self.joined && needs_text) || self.failed || self.sequence.cancelled() {
            return gates;
        }
        let Some(sender) = &self.sender else {
            return gates;
        };
        let previous_inflight = self.inflight;
        if sender.ordered() {
            let durable = self
                .journal
                .metadata
                .as_ref()
                .map_or(&[][..], |metadata| metadata.pending.as_slice());
            let prepared = draft.map_or(&[][..], |draft| draft.pending.as_slice());
            let mut durable = PendingIds::new(durable.iter().map(String::as_str));
            let mut prepared = PendingIds::new(prepared.iter().map(|request| request.id.as_str()));
            for request in self.pending.iter().skip(self.inflight) {
                let gate = if durable.contains(&request.id) {
                    if !sender.try_send_edit(request, &self.sequence, None) {
                        break;
                    }
                    None
                } else if prepared.contains(&request.id) {
                    let Some(permit) = sender.try_reserve_edit(&self.sequence) else {
                        break;
                    };
                    let (gate, ready) = tokio::sync::oneshot::channel();
                    permit.send(request, Some(ready));
                    Some(gate)
                } else {
                    break;
                };
                self.inflight += 1;
                if let Some(gate) = gate {
                    gates.push(gate);
                }
            }
        } else if self.inflight == 0
            || self
                .dispatched
                .is_some_and(|at| at.elapsed().as_secs() >= 5)
        {
            if let Some(request) = self.pending.front()
                && self
                    .journal
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.pending.contains(&request.id))
                && let Ok(permit) = sender.try_reserve()
            {
                permit.send(ClientMessage::Act {
                    id: request.id.clone(),
                    action: engine::actions::Action::ChangeRecord {
                        request: request.as_ref().clone(),
                    },
                });
                self.inflight = 1;
                self.dispatched = Some(Instant::now());
            }
        }
        if self.inflight > previous_inflight && gates.is_empty() {
            self.status = match self.source {
                Source::Local => "Saved locally · syncing…".into(),
                Source::Organ(_) => "Waiting for origin…".into(),
            };
        }
        gates
    }
}
