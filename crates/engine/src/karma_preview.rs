use std::{future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, Utc};
use nucleus::karma::rule_field::{RuleFieldInput, RuleIdentity};
use nucleus::simulation::{
    CheckDefinition, Checking, Coverage, Finding, Quantity, RuleCycle, Stop,
};
use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, actions::Action};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposedRule {
    pub identity: Option<RuleIdentity>,
    pub rule: Option<String>,
    pub expected_revision: Option<i64>,
    pub fields: [RuleFieldInput; 3],
}

impl ProposedRule {
    pub fn action(&self, request_id: String) -> Action {
        Action::SaveKarmaRule {
            identity: self.identity.clone(),
            rule: self.rule.clone(),
            expected_revision: self.expected_revision,
            fields: self.fields.clone(),
            request_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Input {
    Quantity {
        after_ms: u64,
        record: String,
        value: nucleus::DecimalValue,
    },
    Extension {
        after_ms: u64,
        record: String,
        namespace: String,
        value: serde_json::Value,
    },
    Occurrence {
        after_ms: u64,
        proposal: usize,
    },
}

impl Input {
    pub fn after_ms(&self) -> u64 {
        match self {
            Self::Quantity { after_ms, .. }
            | Self::Extension { after_ms, .. }
            | Self::Occurrence { after_ms, .. } => *after_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub horizon_ms: u64,
    pub steps: u64,
    pub rule_evaluations: u64,
    pub wall_time_ms: Option<u64>,
    pub evidence_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            horizon_ms: 86_400_000,
            steps: 10_000,
            rule_evaluations: 100_000,
            wall_time_ms: Some(60_000),
            evidence_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub proposals: Vec<ProposedRule>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub inputs: Vec<Input>,
    #[serde(default)]
    pub records: Vec<String>,
    #[serde(default)]
    pub quantity_basis: nucleus::simulation::QuantityBasis,
    #[serde(default)]
    pub checks: Vec<CheckDefinition>,
    #[serde(default)]
    pub saved_checks: Option<String>,
    #[serde(default)]
    pub checks_start_ms: Option<i64>,
    #[serde(default)]
    pub checking: Checking,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FinalValue {
    pub record: String,
    pub quantity: Option<Quantity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<nucleus::transfer::karma::Snapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    #[serde(skip)]
    #[schemars(skip)]
    pub required_reads: Vec<String>,
    pub draft: String,
    pub source: String,
    pub source_current: bool,
    pub start_ms: i64,
    pub requested_until_ms: i64,
    pub stopped_at_ms: i64,
    pub stop: Stop,
    pub evaluations: u64,
    pub final_values: Vec<FinalValue>,
    pub first_failure: Option<Finding>,
    pub cycles: Vec<RuleCycle>,
    pub coverage: Vec<Coverage>,
    pub incomplete: bool,
    pub unsupported: Vec<String>,
    pub assumptions: Vec<Input>,
}

pub type PreviewFuture<'a> = Pin<Box<dyn Future<Output = Result<Report, EngineError>> + Send + 'a>>;

pub trait Runner: Send + Sync {
    fn run<'a>(
        &'a self,
        engine: &'a Engine,
        actor: Option<String>,
        request: Request,
        now: DateTime<Utc>,
    ) -> PreviewFuture<'a>;
}

pub fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_preview_invalid",
        message: message.to_string(),
    }
}

impl Request {
    pub fn fingerprint(&self) -> Result<String, EngineError> {
        let encoded = serde_json::to_string(self).map_err(EngineError::Json)?;
        Ok(
            nucleus::karma::canonical_hash("lince.karma-proposal.v1", &encoded)
                .map_err(invalid)?
                .as_str()
                .into(),
        )
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.proposals.is_empty()
            || self.proposals.len() > 32
            || self.inputs.len() > 1024
            || self.records.len() > 128
            || self.checks.len() > 1024
            || serde_json::to_vec(self).map_err(EngineError::Json)?.len() > 1024 * 1024
        {
            return Err(invalid(
                "Use 1–32 proposed Rules and a bounded set of inputs and readings",
            ));
        }
        let limits = &self.limits;
        if limits.horizon_ms > i64::MAX as u64
            || limits.steps == 0
            || limits.steps > 10_000_000
            || limits.rule_evaluations == 0
            || limits.rule_evaluations > 10_000_000
            || !(4096..=1024 * 1024 * 1024).contains(&limits.evidence_bytes)
            || limits
                .wall_time_ms
                .is_some_and(|value| value == 0 || value > 43_200_000)
            || self
                .inputs
                .iter()
                .any(|input| input.after_ms() > limits.horizon_ms)
            || self.inputs.iter().any(|input| {
                matches!(input, Input::Occurrence { proposal, .. }
                if *proposal >= self.proposals.len())
            })
        {
            return Err(invalid("Invalid Simulation limits or input time"));
        }
        Ok(())
    }
}

impl Engine {
    pub fn install_karma_preview_runner(&self, runner: Arc<dyn Runner>) -> Result<(), EngineError> {
        *self
            .karma_preview_runner
            .write()
            .map_err(|_| invalid("Preview runner is unavailable"))? = Some(runner);
        Ok(())
    }

    pub(crate) async fn preview_karma_proposal(
        &self,
        request: Request,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Report, EngineError> {
        if nucleus::execution::current()
            .and_then(|execution| execution.control())
            .is_some()
        {
            return Err(invalid("A Simulation cannot start another Simulation"));
        }
        request.validate()?;
        self.require_permission(actor.as_deref(), "record:read")
            .await?;
        self.require_permission(actor.as_deref(), "frequency:read")
            .await?;
        for (index, proposal) in request.proposals.iter().enumerate() {
            self.authorize_action(
                &proposal.action(format!("preview-validation-{index}")),
                actor.as_deref(),
            )
            .await?;
        }
        let runner = self
            .karma_preview_runner
            .read()
            .map_err(|_| invalid("Preview runner is unavailable"))?
            .clone()
            .ok_or_else(|| invalid("This host has not installed the Simulation service"))?;
        let mut report = runner.run(self, actor.clone(), request, now).await?;
        self.access_scope(false, async {
            self.require_permission(actor.as_deref(), "record:read")
                .await?;
            self.require_permission(actor.as_deref(), "frequency:read")
                .await?;
            self.refuse_unreadable_karma_inputs(actor.as_deref(), &report.required_reads)
                .await?;
            report.source_current = self.store.state_hash().await?.as_str() == report.source;
            Ok(report)
        })
        .await
    }
}
