//! Agreement policies, review guards, and signed transitions.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgreementGuard {
    pub level: u8,
    pub change_uid: Option<String>,
}

/// Signed movement between a party’s agreement levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAgreementTransitionEvidence {
    pub action: String,
    pub idempotency_key: String,
    pub transfer_uid: String,
    pub revision: u64,
    pub party_uid: String,
    pub person_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organ_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_fact_hash: Option<String>,
    pub from_level: u8,
    pub to_level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum AgreementType {
    Individual,
    Full,
    Percentage,
    Dependency,
}

impl AgreementType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "individual" => Self::Individual,
            "full" => Self::Full,
            "percentage" => Self::Percentage,
            "dependency" => Self::Dependency,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Full => "full",
            Self::Percentage => "percentage",
            Self::Dependency => "dependency",
        }
    }
}

/// Check participation levels against the Transfer's aggregate agreement policy.
///
/// Levels of two or more count as committed. Dependency readiness is checked separately.
pub fn policy_satisfied(agreement: AgreementType, pct: Option<u8>, party_levels: &[i64]) -> bool {
    match agreement {
        AgreementType::Individual | AgreementType::Dependency => true,
        AgreementType::Full => {
            !party_levels.is_empty() && party_levels.iter().all(|&level| level >= 2)
        }
        AgreementType::Percentage => {
            let pct = usize::from(pct.unwrap_or(100));
            let count = party_levels.len();
            if count == 0 || pct > 100 {
                return false;
            }
            // Split the ceiling calculation so a large party count cannot overflow.
            let needed = count / 100 * pct + (count % 100 * pct).div_ceil(100);
            party_levels.iter().filter(|&&level| level >= 2).count() >= needed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgreementType, policy_satisfied};

    #[test]
    fn policies() {
        assert!(policy_satisfied(AgreementType::Individual, None, &[0, 0]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[2, 1]));
        assert!(policy_satisfied(AgreementType::Full, None, &[2, 2]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[]));
        assert!(!policy_satisfied(
            AgreementType::Percentage,
            Some(50),
            &[2, 0, 0]
        ));
        assert!(policy_satisfied(
            AgreementType::Percentage,
            Some(50),
            &[2, 2, 0]
        ));
        assert!(!policy_satisfied(
            AgreementType::Percentage,
            Some(80),
            &[2, 2, 0]
        ));
        assert!(policy_satisfied(AgreementType::Percentage, Some(0), &[0]));
        assert!(!policy_satisfied(
            AgreementType::Percentage,
            Some(101),
            &[2]
        ));
    }
}
