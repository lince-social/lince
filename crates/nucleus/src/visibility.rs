use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_CONDITIONS: usize = 64;
pub const MAX_ORGANS_PER_CONDITION: usize = 64;
pub const MAX_POLICY_BYTES: usize = 12 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Data {
    Record,
    Place,
    LiveLocation,
}

impl Data {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Place => "place",
            Self::LiveLocation => "live_location",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    pub value: u32,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Condition {
    pub organs: Vec<String>,
    pub lower: Option<Bound>,
    pub upper: Option<Bound>,
}

impl Condition {
    pub fn matches(&self, organ: &str, proximity: Option<u32>) -> bool {
        if organ.is_empty()
            || (!self.organs.is_empty() && !self.organs.iter().any(|uid| uid == organ))
        {
            return false;
        }
        if self.lower.is_none() && self.upper.is_none() {
            return true;
        }
        proximity.is_some_and(|value| {
            self.lower.is_none_or(|bound| {
                value > bound.value || (bound.inclusive && value == bound.value)
            }) && self.upper.is_none_or(|bound| {
                value < bound.value || (bound.inclusive && value == bound.value)
            })
        })
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.organs.len() > MAX_ORGANS_PER_CONDITION {
            return Err("Choose at most 64 Organs in each visibility condition");
        }
        let mut seen = std::collections::HashSet::new();
        if self
            .organs
            .iter()
            .any(|uid| !crate::valid_uid(uid, "r") || !seen.insert(uid))
        {
            return Err("Choose distinct valid Organ identities");
        }
        let minimum = match self.lower {
            Some(Bound {
                value,
                inclusive: false,
            }) => value.checked_add(1),
            Some(Bound { value, .. }) => Some(value),
            None => Some(0),
        };
        let maximum = match self.upper {
            Some(Bound {
                value,
                inclusive: false,
            }) => value.checked_sub(1),
            Some(Bound { value, .. }) => Some(value),
            None => Some(u32::MAX),
        };
        if !minimum.zip(maximum).is_some_and(|(low, high)| low <= high) {
            return Err("Choose a nonempty proximity range");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub include: Vec<Condition>,
    pub exclude: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    pub allowed: bool,
    pub included_by: Vec<usize>,
    pub excluded_by: Vec<usize>,
}

impl Policy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.include.len() + self.exclude.len() > MAX_CONDITIONS {
            return Err("Choose at most 64 visibility conditions");
        }
        for condition in self.include.iter().chain(&self.exclude) {
            condition.validate()?;
        }
        if serde_json::to_vec(self)
            .map_err(|_| "Cannot encode visibility rules")?
            .len()
            > MAX_POLICY_BYTES
        {
            return Err("Visibility rules exceed the 12 KiB limit; use fewer conditions or Organs");
        }
        Ok(())
    }

    pub fn decide(&self, organ: &str, proximity: Option<u32>) -> Decision {
        let matches = |conditions: &[Condition]| {
            conditions
                .iter()
                .enumerate()
                .filter_map(|(index, condition)| {
                    condition.matches(organ, proximity).then_some(index)
                })
                .collect::<Vec<_>>()
        };
        let included_by = matches(&self.include);
        let excluded_by = matches(&self.exclude);
        Decision {
            allowed: !included_by.is_empty() && excluded_by.is_empty(),
            included_by,
            excluded_by,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SavedPolicy {
    pub controller_uid: String,
    pub revision: u64,
    pub policy: Policy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Organ {
    pub uid: String,
    pub label: String,
    pub proximity: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Context {
    pub record_uid: String,
    pub data: Data,
    pub saved: Option<SavedPolicy>,
    pub organs: Vec<Organ>,
    pub records: Vec<crate::location::Choice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Context {
        record_uid: String,
        data: Data,
    },
    Save {
        record_uid: String,
        data: Data,
        policy: Policy,
        expected_revision: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_ranges_intersect_organs_and_exclusions_win() {
        let a = crate::new_uid("r");
        let b = crate::new_uid("r");
        let policy = Policy {
            include: vec![
                Condition {
                    upper: Some(Bound {
                        value: 3,
                        inclusive: false,
                    }),
                    ..Default::default()
                },
                Condition {
                    organs: vec![b.clone()],
                    lower: Some(Bound {
                        value: 6,
                        inclusive: true,
                    }),
                    ..Default::default()
                },
            ],
            exclude: vec![Condition {
                organs: vec![a.clone()],
                lower: Some(Bound {
                    value: 1,
                    inclusive: true,
                }),
                upper: Some(Bound {
                    value: 2,
                    inclusive: true,
                }),
            }],
        };
        policy.validate().unwrap();
        assert!(policy.decide(&a, Some(0)).allowed);
        let denied = policy.decide(&a, Some(2));
        assert_eq!(denied.included_by, vec![0]);
        assert_eq!(denied.excluded_by, vec![0]);
        assert!(!denied.allowed);
        assert!(!policy.decide(&a, Some(3)).allowed);
        assert!(policy.decide(&b, Some(6)).allowed);
        assert!(!policy.decide(&a, Some(6)).allowed);
        assert!(!policy.decide(&b, None).allowed);
        assert!(!policy.decide("", Some(1)).allowed);
        assert!(!Policy::default().decide(&b, Some(0)).allowed);
    }

    #[test]
    fn invalid_ranges_and_oversized_policies_are_rejected() {
        for (lower, upper) in [
            (
                Some(Bound {
                    value: u32::MAX,
                    inclusive: false,
                }),
                None,
            ),
            (
                None,
                Some(Bound {
                    value: 0,
                    inclusive: false,
                }),
            ),
            (
                Some(Bound {
                    value: 3,
                    inclusive: false,
                }),
                Some(Bound {
                    value: 4,
                    inclusive: false,
                }),
            ),
        ] {
            assert!(
                Policy {
                    include: vec![Condition {
                        lower,
                        upper,
                        ..Default::default()
                    }],
                    exclude: vec![]
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Policy {
                include: vec![Condition::default(); MAX_CONDITIONS + 1],
                exclude: vec![]
            }
            .validate()
            .is_err()
        );
        let organs = (0..MAX_ORGANS_PER_CONDITION)
            .map(|_| crate::new_uid("r"))
            .collect::<Vec<_>>();
        assert!(
            Policy {
                include: vec![
                    Condition {
                        organs,
                        ..Default::default()
                    };
                    MAX_CONDITIONS
                ],
                exclude: vec![]
            }
            .validate()
            .is_err()
        );
    }
}
