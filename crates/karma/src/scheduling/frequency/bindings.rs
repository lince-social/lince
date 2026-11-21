//! Parameter domains, override checks, and resolution of typed bindings.

use std::{collections::BTreeMap, num::NonZeroU32};

use super::{
    DurationBinding, FrequencyCompileError as CompileError,
    FrequencyCompileErrorKind::{
        self, InvalidParameterRange, ParameterOutOfRange, ParameterTypeMismatch, UnknownParameter,
    },
    FrequencyParameterDefinition, FrequencyParameterValue, ParameterDefinitions, ParameterValues,
    PositiveIntegerBinding,
};
use crate::{DurationMs, LocalId};

impl FrequencyParameterDefinition {
    pub(super) fn validate(&self, id: &LocalId) -> Result<(), CompileError> {
        let message = match self {
            Self::Duration {
                default,
                minimum,
                maximum,
            } if minimum.get() < 0 || minimum > default || default > maximum => {
                "duration parameter requires 0 <= minimum <= default <= maximum"
            }
            Self::PositiveInteger {
                default,
                minimum,
                maximum,
            } if minimum > default || default > maximum => {
                "positive-integer parameter requires minimum <= default <= maximum"
            }
            _ => return Ok(()),
        };
        Err(parameter_error(InvalidParameterRange, id, message))
    }

    fn effective_value(
        &self,
        id: &LocalId,
        override_value: Option<&FrequencyParameterValue>,
    ) -> Result<FrequencyParameterValue, CompileError> {
        let Some(&value) = override_value else {
            return Ok(match *self {
                Self::Duration { default: value, .. } => {
                    FrequencyParameterValue::Duration { value }
                }
                Self::PositiveInteger { default: value, .. } => {
                    FrequencyParameterValue::PositiveInteger { value }
                }
            });
        };
        let within_range = match (self, &value) {
            (
                Self::Duration {
                    minimum, maximum, ..
                },
                FrequencyParameterValue::Duration { value },
            ) => (minimum..=maximum).contains(&value),
            (
                Self::PositiveInteger {
                    minimum, maximum, ..
                },
                FrequencyParameterValue::PositiveInteger { value },
            ) => (minimum..=maximum).contains(&value),
            _ => {
                return Err(parameter_error(
                    ParameterTypeMismatch,
                    id,
                    "override type does not match Frequency parameter definition",
                ));
            }
        };
        if !within_range {
            return Err(parameter_error(
                ParameterOutOfRange,
                id,
                "override is outside the inclusive Frequency parameter range",
            ));
        }
        Ok(value)
    }
}

pub(super) fn effective_parameters(
    definitions: &ParameterDefinitions,
    overrides: &ParameterValues,
) -> Result<ParameterValues, CompileError> {
    if let Some(id) = overrides.keys().find(|id| !definitions.contains_key(*id)) {
        return Err(parameter_error(
            UnknownParameter,
            id,
            "override names an undeclared Frequency parameter",
        ));
    }

    let mut effective = BTreeMap::new();
    for (id, definition) in definitions {
        let value = definition.effective_value(id, overrides.get(id))?;
        effective.insert(id.clone(), value);
    }
    Ok(effective)
}

fn parameter_error(kind: FrequencyCompileErrorKind, id: &LocalId, message: &str) -> CompileError {
    CompileError::new(kind, format!("/parameters/{}", id.as_str()), message)
}

impl DurationBinding {
    pub(super) fn range(
        &self,
        definitions: &ParameterDefinitions,
        path: &str,
    ) -> Result<(DurationMs, DurationMs), CompileError> {
        match *self {
            Self::Literal { value } => Ok((value, value)),
            Self::Parameter { ref parameter } => {
                BindingKind::Duration.lookup(definitions, parameter, path, |definition| {
                    match *definition {
                        FrequencyParameterDefinition::Duration {
                            minimum, maximum, ..
                        } => Some((minimum, maximum)),
                        _ => None,
                    }
                })
            }
        }
    }

    pub(super) fn resolve(
        &self,
        parameters: &ParameterValues,
        path: &str,
    ) -> Result<DurationMs, CompileError> {
        match *self {
            Self::Literal { value } => Ok(value),
            Self::Parameter { ref parameter } => {
                BindingKind::Duration.lookup(parameters, parameter, path, |value| match *value {
                    FrequencyParameterValue::Duration { value } => Some(value),
                    _ => None,
                })
            }
        }
    }
}

impl PositiveIntegerBinding {
    pub(super) fn validate(
        &self,
        definitions: &ParameterDefinitions,
        path: &str,
    ) -> Result<(), CompileError> {
        if let Self::Parameter { parameter } = self {
            BindingKind::PositiveInteger.lookup(definitions, parameter, path, |definition| {
                matches!(
                    definition,
                    FrequencyParameterDefinition::PositiveInteger { .. }
                )
                .then_some(())
            })?;
        }
        Ok(())
    }

    pub(super) fn resolve(
        &self,
        parameters: &ParameterValues,
        path: &str,
    ) -> Result<NonZeroU32, CompileError> {
        match *self {
            Self::Literal { value } => Ok(value),
            Self::Parameter { ref parameter } => {
                BindingKind::PositiveInteger.lookup(parameters, parameter, path, |value| {
                    match *value {
                        FrequencyParameterValue::PositiveInteger { value } => Some(value),
                        _ => None,
                    }
                })
            }
        }
    }
}

enum BindingKind {
    Duration,
    PositiveInteger,
}

impl BindingKind {
    fn lookup<T, V>(
        self,
        values: &BTreeMap<LocalId, T>,
        id: &LocalId,
        path: &str,
        select: impl FnOnce(&T) -> Option<V>,
    ) -> Result<V, CompileError> {
        let (kind, other) = match self {
            Self::Duration => ("duration", "positive-integer"),
            Self::PositiveInteger => ("positive-integer", "duration"),
        };
        let value = values.get(id).ok_or_else(|| {
            CompileError::new(
                UnknownParameter,
                path,
                format!("{kind} binding names undeclared parameter {id:?}"),
            )
        })?;
        select(value).ok_or_else(|| {
            CompileError::new(
                ParameterTypeMismatch,
                path,
                format!("{kind} binding names a {other} parameter"),
            )
        })
    }
}
