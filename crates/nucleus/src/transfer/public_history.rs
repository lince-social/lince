use super::receipt::{Measurement, Scope, Stage};
use crate::trust_analysis::contacts::Audience;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const NAMESPACE: &str = "lince.transfer-public-history";
pub const DEFAULT_RETENTION_SECONDS: u32 = 365 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Terms {
    pub nonce: String,
    pub measurements: Vec<Measurement>,
    pub stages: Vec<Stage>,
    pub audience: Audience,
    pub retention_seconds: u32,
    pub agreed_quantity: bool,
}

impl Default for Terms {
    fn default() -> Self {
        Self {
            nonce: crate::new_uid("ph"),
            measurements: Vec::new(),
            stages: vec![Stage::Settled, Stage::SettlementStatus, Stage::Cancelled, Stage::Dispute],
            audience: Audience::Public,
            retention_seconds: DEFAULT_RETENTION_SECONDS,
            agreed_quantity: false,
        }
    }
}

impl Terms {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_uid(&self.nonce, "ph") { return Err("Public-history terms require their original private commitment nonce".into()); }
        self.audience.validate()?;
        if self.retention_seconds == 0 || self.measurements.len() > super::receipt::MAX_MEASURES
            || self.stages.is_empty() || self.stages.len() > 8
            || self.stages.iter().collect::<BTreeSet<_>>().len() != self.stages.len() {
            return Err("Choose bounded public-history meanings, distinct stages and a positive retention term".into());
        }
        let mut measurements = BTreeSet::new();
        for measurement in &self.measurements {
            measurement.validate()?;
            if !measurements.insert(measurement.id()?) { return Err("Public-history meanings must not repeat".into()); }
        }
        if self.measurements.is_empty() && self.stages.iter().any(|stage| matches!(stage, Stage::Need | Stage::Giver | Stage::Receiver)) {
            return Err("Public outcome terms require exact reviewed meanings and units".into());
        }
        Ok(())
    }

    pub fn id(&self) -> Result<crate::karma::CanonicalHash, String> {
        self.validate()?;
        crate::karma::canonical_hash("lince.transfer.public-history-terms.v1", self).map_err(|error| error.to_string())
    }

    pub fn permits(&self, scope: &Scope) -> bool {
        self.validate().is_ok()
            && self.audience == scope.audience
            && scope.public_persons.is_empty()
            && (self.agreed_quantity || scope.agreed_quantity.is_none())
            && (self.agreed_quantity || scope.canonical_unit.is_none())
            && scope.measurements.iter().all(|measurement| self.measurements.contains(measurement))
            && scope.stages.iter().all(|stage| self.stages.contains(stage))
            && scope.source.activated_at.checked_add(i64::from(self.retention_seconds)).is_some_and(|expiry| scope.expires_at <= expiry)
    }
}
