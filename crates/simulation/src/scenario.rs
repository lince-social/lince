use std::collections::BTreeSet;

use engine::actions::Action;
use nucleus::simulation::{Predicate, Version};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub version: Version,
    pub name: String,
    pub seed: u64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub limits: Limits,
    pub cells: Vec<Cell>,
    pub inputs: Vec<Input>,
    pub checks: Vec<Check>,
    #[serde(default)]
    pub checking: nucleus::simulation::Checking,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub steps: u64,
    #[serde(default = "default_rule_evaluations")]
    pub rule_evaluations: u64,
    #[serde(default)]
    pub wall_time_ms: Option<u64>,
    pub evidence_bytes: u64,
    pub pending_messages: usize,
}

fn default_rule_evaluations() -> u64 {
    100_000
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 10_000,
            rule_evaluations: default_rule_evaluations(),
            wall_time_ms: None,
            evidence_bytes: 64 * 1024 * 1024,
            pending_messages: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub name: String,
    #[serde(default)]
    pub database: Option<Database>,
    #[serde(default)]
    pub lingua: Vec<LinguaFile>,
    pub seed: Vec<Invocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinguaFile {
    pub name: String,
    pub file: String,
    pub hash: nucleus::karma::CanonicalHash,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Database {
    pub file: String,
    pub hash: nucleus::karma::CanonicalHash,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub id: String,
    pub actor: Option<String>,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub at_ms: i64,
    pub cell: String,
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Event {
    AssumeLoan {
        timing: crate::loans::Timing,
    },
    Action {
        invocation: Invocation,
    },
    Pair {
        peer: String,
    },
    Enrol {
        peer: String,
    },
    Discover {
        peer: String,
    },
    PersonKey {
        person: String,
    },
    AcceptInvitation {
        transfer: String,
        person: String,
        #[serde(default)]
        peer: Option<String>,
    },
    RejectInvitation {
        transfer: String,
        person: String,
        #[serde(default)]
        peer: Option<String>,
    },
    RefreshTransfer {
        transfer: String,
        person: String,
    },
    TransferCommand {
        peer: String,
        transfer: String,
        invocation: Invocation,
        delay_ms: u64,
        copies: u8,
        #[serde(default)]
        duplicate_spacing_ms: u64,
        drop: bool,
    },
    SettleReviewed {
        occurrence: String,
        person: String,
        quantity: nucleus::DecimalValue,
    },
    ApplyReceivedTransfer {
        transfer: String,
        occurrence: String,
        person: String,
        local_record: String,
    },
    AssumeTransfer {
        assumption: crate::assumptions::TransferAssumption,
    },
    TransferDelivery {
        peer: String,
        delay_ms: u64,
        copies: u8,
        #[serde(default)]
        duplicate_spacing_ms: u64,
        drop: bool,
    },
    Sync {
        peer: String,
        delay_ms: u64,
        copies: u8,
        #[serde(default)]
        duplicate_spacing_ms: u64,
        drop: bool,
    },
    Link {
        peer: String,
        connected: bool,
    },
    Restart {},
    Online {
        online: bool,
    },
    Clock {
        offset_ms: i64,
    },
}

pub use nucleus::simulation::CheckDefinition as Check;

fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

impl Scenario {
    pub fn validate(&self) -> crate::Result<()> {
        let invalid = |path: &str| -> crate::Error { format!("invalid scenario: {path}").into() };
        if !name(&self.name) {
            return Err(invalid("name"));
        }
        if self.start_ms < 0
            || self.end_ms < self.start_ms
            || self.end_ms > (i64::MAX >> nucleus::hlc::COUNTER_BITS)
        {
            return Err(invalid("time range"));
        }
        if self.cells.is_empty() || self.cells.len() > 32 {
            return Err(invalid("cells must contain 1–32 Cells"));
        }
        if self.limits.steps == 0
            || self.limits.steps > 10_000_000
            || self.limits.evidence_bytes < 4096
            || self.limits.evidence_bytes > 1024 * 1024 * 1024
            || self.limits.pending_messages > 100_000
            || self.limits.rule_evaluations == 0
            || self.limits.rule_evaluations > 10_000_000
            || self.limits.wall_time_ms.is_some_and(|millis| millis == 0 || millis > 43_200_000)
        {
            return Err(invalid("limits"));
        }
        let mut cells = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for cell in &self.cells {
            let mut files = BTreeSet::new();
            let mut names = BTreeSet::new();
            if cell.lingua.len() > 256 {
                return Err(invalid("a Lingua package may contain at most 256 files"));
            }
            for source in &cell.lingua {
                let path = std::path::Path::new(&source.file);
                let authored = std::path::Path::new(&source.name);
                if source.file.len() > 4096
                    || source.name.len() > 4096
                    || authored
                        .extension()
                        .is_none_or(|extension| extension != "lingua")
                    || authored
                        .components()
                        .any(|component| !matches!(component, std::path::Component::Normal(_)))
                    || path
                        .extension()
                        .is_none_or(|extension| extension != "lingua")
                    || path
                        .components()
                        .any(|component| !matches!(component, std::path::Component::Normal(_)))
                    || !files.insert(&source.file)
                    || !names.insert(&source.name)
                {
                    return Err(invalid(
                        "Lingua files must be unique relative .lingua paths inside the case directory",
                    ));
                }
            }
            if let Some(database) = &cell.database
                && (database.file.is_empty()
                    || std::path::Path::new(&database.file)
                        .components()
                        .any(|component| !matches!(component, std::path::Component::Normal(_))))
            {
                return Err(invalid(
                    "seed database path must stay inside the case directory",
                ));
            }
            if !name(&cell.name) || !cells.insert(&cell.name) {
                return Err(invalid("duplicate or invalid Cell name"));
            }
            for invocation in &cell.seed {
                if !name(&invocation.id) || !ids.insert(invocation.id.clone()) {
                    return Err(invalid("seed invocation id"));
                }
                validate_action(&invocation.action)?;
            }
        }
        for input in &self.inputs {
            if !name(&input.id) || !ids.insert(input.id.clone()) {
                return Err(invalid("input id"));
            }
            if !cells.contains(&input.cell) || !(self.start_ms..=self.end_ms).contains(&input.at_ms)
            {
                return Err(invalid("input Cell or time"));
            }
            match &input.event {
                Event::AssumeLoan { timing } => timing.validate()?,
                Event::Action { invocation } => {
                    if invocation.id != input.id {
                        return Err(invalid("Action invocation id must equal input id"));
                    }
                    validate_action(&invocation.action)?;
                }
                Event::TransferCommand {
                    peer, invocation, ..
                } => {
                    if peer == &input.cell || !cells.contains(peer) || invocation.id != input.id {
                        return Err(invalid("remote command peer or invocation"));
                    }
                    validate_action(&invocation.action)?;
                }
                Event::AcceptInvitation { peer: Some(peer), .. }
                | Event::RejectInvitation { peer: Some(peer), .. } => {
                    if peer == &input.cell || !cells.contains(peer) {
                        return Err(invalid("invitation peer"));
                    }
                }
                Event::Pair { peer }
                | Event::Enrol { peer }
                | Event::Discover { peer }
                | Event::Link { peer, .. }
                | Event::Sync { peer, .. }
                | Event::TransferDelivery { peer, .. } => {
                    if peer == &input.cell || !cells.contains(peer) {
                        return Err(invalid("peer"));
                    }
                }
                Event::Clock { offset_ms } => {
                    if input.at_ms.checked_add(*offset_ms).is_none_or(|time| {
                        time < 0 || time > (i64::MAX >> nucleus::hlc::COUNTER_BITS)
                    }) {
                        return Err(invalid("clock offset"));
                    }
                }
                Event::Restart {}
                | Event::Online { .. }
                | Event::PersonKey { .. }
                | Event::AcceptInvitation { .. }
                | Event::RejectInvitation { .. }
                | Event::RefreshTransfer { .. }
                | Event::ApplyReceivedTransfer { .. }
                | Event::SettleReviewed { .. } => {}
                Event::AssumeTransfer { assumption } => {
                    if assumption.key.is_empty() || assumption.key.len() > 256
                        || assumption.title.len() > 2_000 || !assumption.quantity.is_positive()
                        || assumption.person.is_empty() || assumption.record.is_empty()
                        || assumption.source.as_ref().is_some_and(|source| source.revision == 0 || source.transfer.is_empty() || source.promise.is_empty() || source.exchange.is_empty()) {
                        return Err(invalid("Transfer assumption"));
                    }
                }
            }
            if let Event::Sync {
                delay_ms,
                copies,
                duplicate_spacing_ms,
                ..
            }
            | Event::TransferDelivery {
                delay_ms,
                copies,
                duplicate_spacing_ms,
                ..
            }
            | Event::TransferCommand {
                delay_ms,
                copies,
                duplicate_spacing_ms,
                ..
            } = input.event
                && (copies == 0
                    || copies > 16
                    || duplicate_spacing_ms
                        .checked_mul(u64::from(copies.saturating_sub(1)))
                        .and_then(|span| span.checked_add(delay_ms))
                        .and_then(|span| i64::try_from(span).ok())
                        .and_then(|span| input.at_ms.checked_add(span))
                        .is_none())
            {
                return Err(invalid("message delivery"));
            }
        }
        let mut checks = BTreeSet::new();
        if self.checking.evaluations == 0
            || self.checking.evaluations > 10_000_000
            || self.checks.len() > 1024
        {
            return Err(invalid("check budget"));
        }
        for check in &self.checks {
            if !name(&check.id) || !checks.insert(&check.id) {
                return Err(invalid("check id"));
            }
            let (from, until) = check.interval(self.start_ms, self.end_ms);
            if matches!(
                check.predicate,
                Predicate::FactChain {}
                    | Predicate::OncePerOccurrence {}
                    | Predicate::NoUnexpectedRefusals {}
                    | Predicate::ExpectedRefusal { .. }
                    | Predicate::ExpectedMessageRefusal { .. }
            ) && (from != self.start_ms || until != self.end_ms)
            {
                return Err(invalid(
                    "history checks cover the full run; time windows apply to quantity and convergence checks",
                ));
            }
            if from < self.start_ms
                || until > self.end_ms
                || from > until
                || check.options.name.len() > 200
            {
                return Err(invalid("check window or name"));
            }
            use nucleus::simulation::Evaluation;
            match check.evaluation() {
                Evaluation::At { at_ms } if !(from..=until).contains(&at_ms) => {
                    return Err(invalid("check time"));
                }
                Evaluation::EveryEvents { every: 0 } | Evaluation::EveryDuration { millis: 0 } => {
                    return Err(invalid("check interval"));
                }
                Evaluation::EveryDuration { millis } if millis > i64::MAX as u64 => {
                    return Err(invalid("check interval"));
                }
                _ => {}
            }
            match &check.predicate {
                Predicate::Quantity { cell, record, .. }
                    if !cells.contains(cell) || record.is_empty() =>
                {
                    return Err(invalid("quantity check target"));
                }
                Predicate::QuantityEquals { cell, at_ms, .. } => {
                    if !cells.contains(cell) || !(self.start_ms..=self.end_ms).contains(at_ms) {
                        return Err(invalid("quantity check scope"));
                    }
                }
                Predicate::Nonnegative { cell, .. } if !cells.contains(cell) => {
                    return Err(invalid("check Cell"));
                }
                Predicate::Converged {
                    cells: peers,
                    at_ms,
                    ..
                } => {
                    if peers.len() < 2
                        || peers.iter().any(|peer| !cells.contains(peer))
                        || !(self.start_ms..=self.end_ms).contains(at_ms)
                    {
                        return Err(invalid("convergence scope"));
                    }
                }
                Predicate::ExpectedRefusal { input, .. } if !ids.contains(input) => {
                    return Err(invalid("refusal input"));
                }
                Predicate::ExpectedMessageRefusal { input, copy, .. } => {
                    let valid = self.inputs.iter().any(|candidate| {
                        candidate.id == *input
                            && matches!(candidate.event,
                                Event::Sync { copies, .. } | Event::TransferDelivery { copies, .. } | Event::TransferCommand { copies, .. }
                                if *copy < copies)
                    });
                    if !valid {
                        return Err(invalid("refused message copy"));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn validate_action(action: &Action) -> crate::Result<()> {
    match action {
        Action::CreateRecord { quantity, .. }
        | Action::SetQuantity {
            value: quantity, ..
        }
        | Action::AddQuantity {
            delta: quantity, ..
        } if !quantity.is_finite() => Err("nonfinite action quantity".into()),
        Action::CreateRecord { .. }
        | Action::SetQuantity { .. }
        | Action::SetQuantityExact { .. }
        | Action::AddQuantityExact { .. }
        | Action::AddQuantityGroupExact { .. }
        | Action::AddQuantity { .. }
        | Action::SetSlug { .. }
        | Action::SetExtension { .. }
        | Action::DeleteRecord { .. }
        | Action::CreateFrequency { .. }
        | Action::DeleteFrequency { .. }
        | Action::CreateRecurrence { .. }
        | Action::ReviseRecurrence { .. }
        | Action::DeleteRecurrence { .. }
        | Action::SetRecurrencePaused { .. }
        | Action::ApplyRecurrenceOccurrence { .. }
        | Action::SaveKarmaRule { .. }
        | Action::SaveKarmaSchedule { .. }
        | Action::InspectKarmaSchedules { .. }
        | Action::PreviewKarmaScheduleDates { .. }
        | Action::PreviewKarmaHabit { .. }
        | Action::ImportKarmaHabit { .. }
        | Action::CancelKarmaSchedule { .. }
        | Action::RetryKarmaSchedule { .. }
        | Action::ReviseKarmaField { .. }
        | Action::PreviewKarmaReading { .. } => Ok(()),
        Action::SetContactTrust { .. }
        | Action::SetSyncPolicy { .. }
        | Action::SetContactScope { .. }
        | Action::CreateLingua { .. }
        | Action::CreateConcept { .. }
        | Action::SetUnit { .. }
        | Action::HideRecordFromContact { .. }
        | Action::GrantVisibility { .. }
        | Action::GrantPermission { .. }
        | Action::RevokePermission { .. }
        | Action::RevokeKarmaGrant { .. }
        | Action::CreateKarmaGrant { .. }
        | Action::NarrowKarmaGrant { .. }
        | Action::ActivateKarmaGrant { .. }
        | Action::CreateKarmaProgram { .. }
        | Action::ReviseKarmaProgram { .. }
        | Action::ActivateKarmaProgram { .. }
        | Action::PauseKarmaProgram { .. }
        | Action::SetKarmaExecution { .. }
        | Action::RespondKarmaCandidate { .. } => Ok(()),
        Action::CreateTransferDraft { .. }
        | Action::CreateTransferThread { .. }
        | Action::CreateTransferMessage { .. }
        | Action::ReviseTransferDraft { .. }
        | Action::CounterofferTransfer { .. }
        | Action::ClaimOpenTransferPromise { .. }
        | Action::AddressTransferInvitation { .. }
        | Action::AcceptTransferInvitation { .. }
        | Action::RejectTransferInvitation { .. }
        | Action::SetTransferAgreementLevel { .. }
        | Action::AssignTransferAgreementLevel { .. }
        | Action::PublishTransfer { .. }
        | Action::ActivateTransferFulfillment { .. }
        | Action::ActivateTransferOccurrence { .. }
        | Action::SetTransferOccurrenceClaim { .. }
        | Action::SetTransferOccurrenceDispute { .. }
        | Action::CompensateTransferApplication { .. }
        | Action::CompensateTransferOccurrenceSettlement { .. }
        | Action::SettleTransferOccurrence { .. }
        | Action::BeginTransferSettlement { .. }
        | Action::SetTransferPrivateApplicationPolicy { .. }
        | Action::ProposeTransferLoanExtension { .. }
        | Action::SetRecordStockLimit { .. }
        | Action::SetTransferChildRequirement { .. }
        | Action::ProposeTransferCancellation { .. }
        | Action::ApplyTransferCancellation { .. }
        | Action::ApplyTransferApplication { .. }
        | Action::ConfigureTransferDelivery { .. }
        | Action::EnqueueTransferDelivery { .. }
        | Action::RetryTransferDelivery { .. }
        | Action::RevokeTransferDelivery { .. }
        | Action::RefreshTransferDelivery { .. } => Ok(()),
        _ => Err(
            "Action requires an environment adapter that this scenario runner does not provide"
                .into(),
        ),
    }
}
