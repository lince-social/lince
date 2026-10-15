use super::{Operator, gpu_quantity};
use crate::DecimalValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumericBinding {
    pub slot: u8,
    pub label: String,
    pub source: NumericSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumericSource {
    pub organ: Option<String>,
    pub record: Option<String>,
    pub field: NumericField,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NumericField {
    Quantity,
    Extension {
        namespace: String,
        path: Vec<String>,
    },
}

pub fn bounded(bindings: &[NumericBinding]) -> bool {
    bindings.len() <= 8
        && bindings.iter().all(|binding| {
            binding.label.len() <= 512
                && binding
                    .source
                    .organ
                    .as_ref()
                    .is_none_or(|uid| uid.len() <= 512)
                && binding
                    .source
                    .record
                    .as_ref()
                    .is_none_or(|uid| uid.len() <= 512)
                && match &binding.source.field {
                    NumericField::Quantity => true,
                    NumericField::Extension { namespace, path } => {
                        namespace.len() <= 512
                            && path.len() <= 1024
                            && path.iter().all(|part| part.len() <= 4096)
                            && path.iter().map(String::len).sum::<usize>() <= 4096
                    }
                }
        })
}

impl Default for NumericSource {
    fn default() -> Self {
        Self {
            organ: None,
            record: None,
            field: NumericField::Quantity,
        }
    }
}

pub fn validate(bindings: &[NumericBinding]) -> Result<(), String> {
    if !bounded(bindings) {
        return Err("Use at most eight bounded numeric inputs.".into());
    }
    let mut slots = BTreeSet::new();
    for binding in bindings {
        if binding.slot >= 8 || !slots.insert(binding.slot) {
            return Err("Numeric input slots must be unique numbers from 0 to 7.".into());
        }
        if binding.label.trim().is_empty()
            || binding.label.len() > 64
            || binding.label.chars().any(char::is_control)
        {
            return Err("Numeric input labels use 1–64 bytes of readable text.".into());
        }
        if binding
            .source
            .organ
            .as_ref()
            .is_some_and(|uid| !crate::valid_uid(uid, "r"))
            || binding
                .source
                .record
                .as_ref()
                .is_some_and(|uid| !crate::valid_uid(uid, "r"))
        {
            return Err("Choose stable Organ and Record UIDs for numeric inputs.".into());
        }
        if let NumericField::Extension { namespace, path } = &binding.source.field {
            if namespace.is_empty()
                || namespace.len() > 128
                || namespace.chars().any(char::is_control)
                || namespace.starts_with("lince.social.")
                || path.is_empty()
                || path.len() > 8
                || path.iter().any(|part| {
                    part.is_empty() || part.len() > 128 || part.chars().any(char::is_control)
                })
                || path.iter().map(String::len).sum::<usize>() > 512
            {
                return Err(
                    "Choose a readable extension namespace and a bounded field path.".into(),
                );
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumericValues {
    decimals: [Option<String>; 8],
}

impl NumericValues {
    pub fn resolve(
        bindings: &[NumericBinding],
        mut read: impl FnMut(&NumericSource) -> Option<String>,
    ) -> Result<Self, String> {
        validate(bindings)?;
        let mut values = Self::default();
        for binding in bindings {
            values.decimals[usize::from(binding.slot)] = read(&binding.source)
                .filter(|raw| raw.len() <= 128 && DecimalValue::parse_inferred(raw).is_ok());
        }
        Ok(values)
    }

    pub fn get(&self, slot: u8) -> Option<&str> {
        self.decimals.get(usize::from(slot))?.as_deref()
    }

    pub fn gpu(&self) -> ([f32; 8], [u32; 8]) {
        let mut values = [0.0; 8];
        let mut present = [0; 8];
        for slot in 0..8 {
            if let Some(value) = gpu_quantity(self.decimals[slot].as_deref()) {
                values[slot] = value;
                present[slot] = 1;
            }
        }
        (values, present)
    }

    pub fn compare(&self, slot: u8, operator: Operator, threshold: &str) -> Option<bool> {
        if threshold.len() > 128 {
            return None;
        }
        let value = DecimalValue::parse_inferred(self.get(slot)?).ok()?;
        let threshold = DecimalValue::parse_inferred(threshold).ok()?;
        let ordering = value.exact_numeric_cmp(threshold);
        Some(match operator {
            Operator::Less => ordering.is_lt(),
            Operator::AtMost => !ordering.is_gt(),
            Operator::Equal => ordering.is_eq(),
            Operator::AtLeast => !ordering.is_lt(),
            Operator::Greater => ordering.is_gt(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(slot: u8) -> NumericBinding {
        NumericBinding {
            slot,
            label: format!("Value {slot}"),
            source: NumericSource::default(),
        }
    }

    #[test]
    fn exact_numeric_conditions_precede_gpu_rounding_and_absence_is_not_zero() {
        let values = NumericValues::resolve(&[binding(0), binding(1), binding(7)], |_| {
            Some("16777217".into())
        })
        .unwrap();
        assert_eq!(values.compare(0, Operator::Greater, "16777216"), Some(true));
        let (gpu, present) = values.gpu();
        assert_eq!(gpu[0], 16777216.0);
        assert_eq!(present[0], 1);
        assert_eq!(present[2], 0);
        assert_eq!(values.compare(2, Operator::Equal, "0"), None);
        let zero = NumericValues::resolve(&[binding(2)], |_| Some("0".into())).unwrap();
        assert_eq!(zero.gpu().1[2], 1);
        assert_eq!(zero.compare(2, Operator::Equal, "0"), Some(true));
        let unavailable = NumericValues::resolve(&[binding(2)], |_| Some("NaN".into())).unwrap();
        assert_eq!(unavailable.get(2), None);
        assert_eq!(unavailable.gpu().1[2], 0);
    }

    #[test]
    fn invalid_or_unbounded_bindings_are_rejected_before_reading_sources() {
        for bindings in [
            vec![binding(8)],
            vec![binding(0), binding(0)],
            (0..9).map(binding).collect(),
        ] {
            assert!(
                NumericValues::resolve(&bindings, |_| panic!(
                    "invalid bindings must not read data"
                ))
                .is_err()
            );
        }
        let mut value = binding(0);
        value.source.record = Some("a-slug-is-not-a-uid".into());
        assert!(validate(&[value.clone()]).is_err());
        value.source.record = Some(crate::new_uid("r"));
        value.source.field = NumericField::Extension {
            namespace: "lince.social.private".into(),
            path: vec!["amount".into()],
        };
        assert!(validate(&[value.clone()]).is_err());
        value.source.field = NumericField::Extension {
            namespace: "nutrition".into(),
            path: vec!["daily".into(), "protein".into()],
        };
        assert!(validate(&[value.clone()]).is_ok());
        let restored: NumericBinding =
            serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
        assert_eq!(restored, value);
        let mut effect = super::super::Effect::default();
        effect.bindings = vec![value.clone()];
        assert!(effect.validate().is_ok());
        let restored: super::super::Effect =
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(restored.bindings, vec![value.clone()]);
        effect.bindings.push(value);
        assert!(effect.validate().is_err());
    }
}
