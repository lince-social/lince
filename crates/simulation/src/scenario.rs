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
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub steps: u64,
    pub evidence_bytes: u64,
    pub pending_messages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 10_000,
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
    },
    SettleReviewed {
        occurrence: String,
        person: String,
        quantity: nucleus::DecimalValue,
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub predicate: Predicate,
}

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
                Event::Action { invocation } => {
                    if invocation.id != input.id {
                        return Err(invalid("Action invocation id must equal input id"));
                    }
                    validate_action(&invocation.action)?;
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
                | Event::SettleReviewed { .. } => {}
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
        for check in &self.checks {
            if !name(&check.id) || !checks.insert(&check.id) {
                return Err(invalid("check id"));
            }
            match &check.predicate {
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
                                Event::Sync { copies, .. } | Event::TransferDelivery { copies, .. }
                                if *copy < copies)
                    });
                    if !valid {
                        return Err(invalid("refused message copy"));
                    }
                }
                _ => {}
            }
        }
        if self.checks.is_empty() {
            return Err(invalid("at least one required check"));
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
        | Action::AddQuantity { .. }
        | Action::SetSlug { .. }
        | Action::DeleteRecord { .. }
        | Action::CreateFrequency { .. }
        | Action::DeleteFrequency { .. }
        | Action::CreateRecurrence { .. }
        | Action::ReviseRecurrence { .. }
        | Action::DeleteRecurrence { .. }
        | Action::SetRecurrencePaused { .. }
        | Action::SaveKarmaRule { .. }
        | Action::ReviseKarmaField { .. }
        | Action::PreviewKarmaReading { .. } => Ok(()),
        Action::SetContactTrust { .. }
        | Action::SetSyncPolicy { .. }
        | Action::SetContactScope { .. }
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
        | Action::ReviseTransferDraft { .. }
        | Action::AddressTransferInvitation { .. }
        | Action::AcceptTransferInvitation { .. }
        | Action::RejectTransferInvitation { .. }
        | Action::SetTransferAgreementLevel { .. }
        | Action::ActivateTransferOccurrence { .. }
        | Action::SetTransferOccurrenceClaim { .. }
        | Action::SettleTransferOccurrence { .. }
        | Action::ConfigureTransferDelivery { .. }
        | Action::EnqueueTransferDelivery { .. }
        | Action::RetryTransferDelivery { .. }
        | Action::RevokeTransferDelivery { .. } => Ok(()),
        _ => Err(
            "Action requires an environment adapter that this scenario runner does not provide"
                .into(),
        ),
    }
}
