//! Node operations and their input and trigger sources.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ExpressionAst, StateContract};
use crate::{CandidateRoute, DurationMs, LiteralValue, LocalId, ResolvedReference, Slug};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum NodeOperation {
    Trigger {
        source: TriggerSource,
        output: LocalId,
    },
    Input {
        source: InputSource,
        output: LocalId,
    },
    Derive {
        expressions: BTreeMap<LocalId, ExpressionAst>,
    },
    Delay {
        input: LocalId,
        output: LocalId,
        initial: LiteralValue,
        state: StateContract,
    },
    Threshold {
        input: LocalId,
        active: LocalId,
        entered: LocalId,
        left: LocalId,
        direction: ThresholdDirection,
        enter: LiteralValue,
        exit: LiteralValue,
        initial_active: bool,
        state: StateContract,
    },
    Debounce {
        input: LocalId,
        stable: LocalId,
        entered: LocalId,
        left: LocalId,
        for_at_least: DurationMs,
        initial: bool,
        state: StateContract,
    },
    Cooldown {
        input: LocalId,
        allowed: LocalId,
        cooldown: DurationMs,
        state: StateContract,
    },
    RateLimit {
        input: LocalId,
        allowed: LocalId,
        max: u32,
        window: DurationMs,
        state: StateContract,
    },
    RouteCandidate {
        condition: LocalId,
        output: LocalId,
        route: CandidateRoute,
        template: Slug,
        fields: BTreeMap<LocalId, LocalId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum ThresholdDirection {
    Above,
    Below,
}

impl NodeOperation {
    pub const fn is_state_boundary(&self) -> bool {
        matches!(self, Self::Delay { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum TriggerSource {
    Manual,
    Fact {
        #[serde(skip_serializing_if = "Option::is_none")]
        record: Option<ResolvedReference>,
        #[serde(skip_serializing_if = "Option::is_none")]
        concept: Option<ResolvedReference>,
    },
    Frequency {
        frequency: ResolvedReference,
    },
    Signal {
        signal: ResolvedReference,
    },
    Decision {
        decision: ResolvedReference,
    },
    Receipt {
        receipt: ResolvedReference,
    },
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum InputSource {
    Parameter { parameter: LocalId },
    RecordQuantity { record: ResolvedReference },
    SavedProtein { view: ResolvedReference },
    Signal { signal: ResolvedReference },
    CapturedFact { fact: ResolvedReference },
}
