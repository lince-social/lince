use super::{GestureCompletion, GestureProgress, GestureResult};
use crate::host::OutputId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GestureContext {
    pub sequence: u64,
    pub owner: u64,
    pub workspace: u64,
    pub output: OutputId,
    pub fingers: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureDelivery {
    Begin(GestureContext),
    Pan(GestureProgress),
    Finish(GestureCompletion),
}

struct Pending {
    context: GestureContext,
    began: bool,
    closed: bool,
    progress: Option<GestureProgress>,
    completion: Option<GestureCompletion>,
}

#[derive(Default)]
pub struct GestureDeliveryBuffer {
    last_sequence: u64,
    pending: Option<Pending>,
    inflight: Option<(u64, GestureDelivery)>,
}

impl GestureDeliveryBuffer {
    pub fn begin(&mut self, context: GestureContext) -> bool {
        if self.pending.is_some()
            || self.inflight.is_some()
            || context.sequence <= self.last_sequence
            || context.workspace == 0
            || !matches!(context.fingers, 3 | 4)
        {
            return false;
        }
        self.last_sequence = context.sequence;
        self.pending = Some(Pending {
            context,
            began: false,
            closed: false,
            progress: None,
            completion: None,
        });
        true
    }

    pub fn progress(&mut self, progress: GestureProgress) -> bool {
        let Some(pending) = self.pending.as_mut().filter(|pending| {
            !pending.closed
                && pending.context.sequence == progress.sequence
                && pending.context.output == progress.output
                && pending.context.fingers == 3
        }) else {
            return false;
        };
        if !progress.horizontal_travel.is_finite() || progress.horizontal_travel.abs() > 1_000_000.0
        {
            self.cancel();
            return false;
        }
        pending.progress = Some(progress);
        true
    }

    pub fn finish(&mut self, mut completion: GestureCompletion) -> bool {
        let Some(pending) = self.pending.as_mut().filter(|pending| {
            !pending.closed
                && pending.context.sequence == completion.sequence
                && pending.context.output == completion.output
        }) else {
            return false;
        };
        let valid = match completion.result {
            GestureResult::Noop | GestureResult::Cancelled => true,
            GestureResult::Pan(travel) => {
                pending.context.fingers == 3 && travel.is_finite() && travel.abs() <= 1_000_000.0
            }
            GestureResult::Workspace(offset) => {
                pending.context.fingers == 3 && matches!(offset, -1 | 1)
            }
            GestureResult::Overview(_) => pending.context.fingers == 4,
        };
        if !valid {
            completion.result = GestureResult::Cancelled;
        }
        pending.closed = true;
        pending.progress = None;
        pending.completion = Some(completion);
        true
    }

    pub fn cancel(&mut self) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if !pending.began {
            self.pending = None;
            return;
        }
        if matches!(self.inflight, Some((_, GestureDelivery::Finish(_)))) {
            return;
        }
        pending.closed = true;
        pending.progress = None;
        pending.completion = Some(GestureCompletion {
            sequence: pending.context.sequence,
            output: pending.context.output,
            result: GestureResult::Cancelled,
        });
    }

    pub fn next_delivery(&self) -> Option<GestureDelivery> {
        if self.inflight.is_some() {
            return None;
        }
        let pending = self.pending.as_ref()?;
        if !pending.began {
            Some(GestureDelivery::Begin(pending.context))
        } else if let Some(completion) = pending.completion {
            Some(GestureDelivery::Finish(completion))
        } else {
            pending.progress.map(GestureDelivery::Pan)
        }
    }

    pub fn sent(&mut self, serial: u64) -> bool {
        if serial == 0 {
            return false;
        }
        let Some(delivery) = self.next_delivery() else {
            return false;
        };
        let pending = self.pending.as_mut().unwrap();
        match delivery {
            GestureDelivery::Begin(_) => pending.began = true,
            GestureDelivery::Pan(_) => pending.progress = None,
            GestureDelivery::Finish(_) => pending.completion = None,
        }
        self.inflight = Some((serial, delivery));
        true
    }

    pub fn acknowledge(&mut self, serial: u64) {
        if self
            .inflight
            .as_ref()
            .is_some_and(|(sent, _)| *sent <= serial)
        {
            if matches!(self.inflight, Some((_, GestureDelivery::Finish(_)))) {
                self.pending = None;
            }
            self.inflight = None;
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.inflight.is_some()
    }
}

#[cfg(test)]
mod tests;
