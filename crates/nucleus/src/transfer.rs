//! Transfer agreement policies (blueprint VIII.2), pure.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Is the transfer's agreement policy satisfied? Levels: 0 none/invalidated,
/// 1 reviewed, 2 committed. `Individual` binds each party only to its own
/// promises, so the bundle-level gate is always open; per-promise checks are
/// the engine's job. `Dependency` is resolved by the engine through the
/// bundled promises' conditions (upstream transfers), not here.
pub fn policy_satisfied(
    agreement: AgreementType,
    pct: Option<u8>,
    party_levels: &[i64],
) -> bool {
    let n = party_levels.len();
    let committed = party_levels.iter().filter(|&&l| l >= 2).count();
    match agreement {
        AgreementType::Individual => true,
        AgreementType::Full => n > 0 && committed == n,
        AgreementType::Percentage => {
            let pct = pct.unwrap_or(100) as usize;
            n > 0 && committed * 100 >= n * pct
        }
        AgreementType::Dependency => true, // engine checks promise conditions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies() {
        assert!(policy_satisfied(AgreementType::Individual, None, &[0, 0]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[2, 1]));
        assert!(policy_satisfied(AgreementType::Full, None, &[2, 2]));
        assert!(!policy_satisfied(AgreementType::Full, None, &[]));
        // ceil semantics: 50% of 3 parties needs 2 committed
        assert!(!policy_satisfied(AgreementType::Percentage, Some(50), &[2, 0, 0]));
        assert!(policy_satisfied(AgreementType::Percentage, Some(50), &[2, 2, 0]));
        assert!(!policy_satisfied(AgreementType::Percentage, Some(80), &[2, 2, 0]));
    }
}
