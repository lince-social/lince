use std::{cmp::Ordering, collections::BTreeMap};

use crate::{DurationMs, LateEventPolicy, LiteralValue, LocalId, ThresholdDirection, TimestampMs};

use super::{
    ControlState::{self, Cooldown, Debounce, RateLimit, Threshold},
    EvaluationError, EvaluationErrorCode, FrozenEvaluationContext, FuelMeter, Site,
    node::{EvaluatedNode, bool_input},
};

pub(super) struct Controls<'a> {
    pub(super) inputs: &'a BTreeMap<LocalId, LiteralValue>,
    pub(super) context: &'a FrozenEvaluationContext,
    pub(super) site: Site<'a>,
    pub(super) meter: &'a FuelMeter,
}

impl Controls<'_> {
    pub(super) fn threshold(
        &self,
        input: &LocalId,
        outputs: [&LocalId; 3],
        direction: ThresholdDirection,
        [enter, exit]: [&LiteralValue; 2],
        initial: bool,
    ) -> Result<EvaluatedNode, EvaluationError> {
        let was_active = match self.state() {
            None => initial,
            Some(Threshold { active }) => *active,
            Some(_) => return Err(self.wrong_state("threshold")),
        };
        let value = self.inputs.get(input).ok_or_else(|| {
            self.meter
                .invariant(self.site, "Proof accepted a missing threshold input")
        })?;
        let limit = if was_active { exit } else { enter };
        let crossing = match direction {
            ThresholdDirection::Above => Ordering::Greater,
            ThresholdDirection::Below => Ordering::Less,
        };
        let active = value.semantic_cmp(limit).map_or(was_active, |order| {
            order == crossing || (!was_active && order == Ordering::Equal)
        });
        Ok(EvaluatedNode::control(
            transition(outputs, was_active, active),
            Threshold { active: was_active },
            Some(Threshold { active }),
        ))
    }

    pub(super) fn debounce(
        &self,
        input: &LocalId,
        outputs: [&LocalId; 3],
        duration: DurationMs,
        initial: bool,
        late_event: LateEventPolicy,
    ) -> Result<EvaluatedNode, EvaluationError> {
        let (was_stable, pending, pending_since, last_observed_at) = match self.state() {
            None => (initial, None, None, None),
            Some(Debounce {
                stable,
                pending,
                pending_since,
                last_observed_at,
            }) => (*stable, *pending, *pending_since, *last_observed_at),
            Some(_) => return Err(self.wrong_state("debounce")),
        };
        if pending.is_some() != pending_since.is_some()
            || pending == Some(was_stable)
            || pending_since > last_observed_at
        {
            return Err(self.invalid_state("debounce control state is internally inconsistent"));
        }
        let before = Debounce {
            stable: was_stable,
            pending,
            pending_since,
            last_observed_at,
        };
        let now = self.logical_time()?;
        if self.ignore_late(now, last_observed_at, late_event)? {
            return Ok(EvaluatedNode::control(
                transition(outputs, was_stable, was_stable),
                before,
                None,
            ));
        }

        let observed = bool_input(self.inputs, input, self.site, self.meter)?;
        let (stable, pending, pending_since) = if observed == was_stable {
            (was_stable, None, None)
        } else if pending != Some(observed) {
            if duration == DurationMs::ZERO {
                (observed, None, None)
            } else {
                (was_stable, Some(observed), Some(now))
            }
        } else {
            let since = pending_since.ok_or_else(|| {
                self.invalid_state("debounce pending state has no pending_since timestamp")
            })?;
            if self.elapsed(now, since)? >= duration.get() {
                (observed, None, None)
            } else {
                (was_stable, pending, Some(since))
            }
        };
        Ok(EvaluatedNode::control(
            transition(outputs, was_stable, stable),
            before,
            Some(Debounce {
                stable,
                pending,
                pending_since,
                last_observed_at: Some(now),
            }),
        ))
    }

    pub(super) fn cooldown(
        &self,
        input: &LocalId,
        output: &LocalId,
        cooldown: DurationMs,
        late_event: LateEventPolicy,
    ) -> Result<EvaluatedNode, EvaluationError> {
        let (last_allowed_at, last_observed_at) = match self.state() {
            None => (None, None),
            Some(Cooldown {
                last_allowed_at,
                last_observed_at,
            }) => (*last_allowed_at, *last_observed_at),
            Some(_) => return Err(self.wrong_state("cooldown")),
        };
        if last_allowed_at > last_observed_at {
            return Err(self.invalid_state("cooldown control state is internally inconsistent"));
        }
        let before = Cooldown {
            last_allowed_at,
            last_observed_at,
        };
        let now = self.logical_time()?;
        if self.ignore_late(now, last_observed_at, late_event)? {
            return Ok(EvaluatedNode::control(
                boolean_port(output, false),
                before,
                None,
            ));
        }
        let fired = bool_input(self.inputs, input, self.site, self.meter)?;
        let eligible = match last_allowed_at {
            None => true,
            Some(previous) => self.elapsed(now, previous)? >= cooldown.get(),
        };
        let allowed = fired && eligible;
        Ok(EvaluatedNode::control(
            boolean_port(output, allowed),
            before,
            Some(Cooldown {
                last_allowed_at: if allowed { Some(now) } else { last_allowed_at },
                last_observed_at: Some(now),
            }),
        ))
    }

    pub(super) fn rate_limit(
        &self,
        input: &LocalId,
        output: &LocalId,
        max: u32,
        window: DurationMs,
        late_event: LateEventPolicy,
    ) -> Result<EvaluatedNode, EvaluationError> {
        let (accepted_at, last_observed_at) = match self.state() {
            None => (&[][..], None),
            Some(RateLimit {
                accepted_at,
                last_observed_at,
            }) => (accepted_at.as_slice(), *last_observed_at),
            Some(_) => return Err(self.wrong_state("rate-limit")),
        };
        let max = usize::try_from(max).unwrap_or(usize::MAX);
        if accepted_at.len() > max
            || !accepted_at.is_sorted()
            || accepted_at.last().copied() > last_observed_at
        {
            return Err(self.invalid_state("rate-limit control state is internally inconsistent"));
        }
        let now = self.logical_time()?;
        let before = || RateLimit {
            accepted_at: accepted_at.to_vec(),
            last_observed_at,
        };
        if self.ignore_late(now, last_observed_at, late_event)? {
            return Ok(EvaluatedNode::control(
                boolean_port(output, false),
                before(),
                None,
            ));
        }
        let mut retained = Vec::with_capacity(accepted_at.len());
        for &accepted in accepted_at {
            if self.elapsed(now, accepted)? < window.get() {
                retained.push(accepted);
            }
        }
        let fired = bool_input(self.inputs, input, self.site, self.meter)?;
        let allowed = fired && retained.len() < max;
        if allowed {
            retained.push(now);
        }
        Ok(EvaluatedNode::control(
            boolean_port(output, allowed),
            before(),
            Some(RateLimit {
                accepted_at: retained,
                last_observed_at: Some(now),
            }),
        ))
    }

    fn state(&self) -> Option<&ControlState> {
        self.context.control_state.get(self.site.node)
    }

    fn logical_time(&self) -> Result<TimestampMs, EvaluationError> {
        self.context.logical_at.ok_or_else(|| {
            self.meter.error(
                EvaluationErrorCode::MissingLogicalTime,
                self.site,
                "temporal control requires a frozen logical timestamp",
            )
        })
    }

    fn ignore_late(
        &self,
        now: TimestampMs,
        previous: Option<TimestampMs>,
        policy: LateEventPolicy,
    ) -> Result<bool, EvaluationError> {
        if previous.is_none_or(|previous| now >= previous) {
            return Ok(false);
        }
        match policy {
            LateEventPolicy::Ignore => Ok(true),
            LateEventPolicy::Reject => Err(self.meter.error(
                EvaluationErrorCode::NonMonotonicLogicalTime,
                self.site,
                "logical time precedes the control state's last observation",
            )),
            LateEventPolicy::Recompute | LateEventPolicy::Compensate => Err(self.meter.error(
                EvaluationErrorCode::ReplayRequired,
                self.site,
                "late-event policy requires ordered occurrence replay",
            )),
        }
    }

    fn elapsed(&self, now: TimestampMs, previous: TimestampMs) -> Result<i64, EvaluationError> {
        now.as_millis()
            .checked_sub(previous.as_millis())
            .filter(|elapsed| *elapsed >= 0)
            .ok_or_else(|| {
                self.invalid_state("control state contains a future or unrepresentable timestamp")
            })
    }

    fn wrong_state(&self, expected: &str) -> EvaluationError {
        self.invalid_state(format!("control state does not match {expected} operation"))
    }

    fn invalid_state(&self, message: impl Into<String>) -> EvaluationError {
        self.meter
            .error(EvaluationErrorCode::RuntimeTypeMismatch, self.site, message)
    }
}

fn boolean_port(port: &LocalId, value: bool) -> BTreeMap<LocalId, LiteralValue> {
    BTreeMap::from([(port.clone(), LiteralValue::Bool { value })])
}

fn transition(
    ports: [&LocalId; 3],
    before: bool,
    current: bool,
) -> BTreeMap<LocalId, LiteralValue> {
    let states = [current, !before && current, before && !current];
    ports
        .into_iter()
        .zip(states)
        .map(|(port, value)| (port.clone(), LiteralValue::Bool { value }))
        .collect()
}
