use crate::{error::NucleusError, valid_uid};

use super::{
    super::{CanonicalHash, transfer_consequence::level},
    Consequence, Consequences,
};

impl Consequence {
    fn validate(&self) -> Result<(), NucleusError> {
        if let Self::ReopenTransferNeed {
            target: Some(target),
            ..
        } = self
            && (target.is_negative() || target.is_zero())
        {
            return Err(NucleusError::Parse(
                "Choose a positive target quantity".into(),
            ));
        }
        if let Self::AdvanceAudience { plan, commitment } = self
            && (!valid_uid(plan, "r") || CanonicalHash::parse(commitment.clone()).is_err())
        {
            return Err(NucleusError::Parse(
                "Choose a reviewed audience plan".into(),
            ));
        }
        if let Self::ShowComponent { component } = self {
            component.validate().map_err(NucleusError::Parse)?;
        }
        if let Some((target, person)) = self.transfer_references() {
            if target.trim().is_empty() || person.trim().is_empty() {
                return Err(NucleusError::Parse(
                    "Choose a Transfer and acting Person".into(),
                ));
            }
            if let Self::SetTransferAgreement {
                level: agreement_level,
                after_ms,
                ..
            } = self
            {
                if let Some(value) = agreement_level {
                    level(*value).map_err(NucleusError::Parse)?;
                }
                if after_ms.is_some_and(|value| value.get() <= 0) {
                    return Err(NucleusError::Parse(
                        "Agreement delay must be positive".into(),
                    ));
                }
            }
            if let Self::ActivateTransferFulfillment {
                promise,
                fulfillment,
                ..
            } = self
                && (promise.trim().is_empty()
                    || fulfillment.trim().is_empty()
                    || fulfillment.len() > 200
                    || fulfillment.chars().any(char::is_control))
            {
                return Err(NucleusError::Parse("Choose a promise and fulfillment key of 1–200 bytes without control characters".into()));
            }
        }
        let concept = match self {
            Self::SetConcept { concept }
            | Self::AddConcept { concept }
            | Self::RemoveConcept { concept } => Some(concept),
            Self::SetQuantityWhere { assertion, .. } => Some(assertion),
            Self::CaptureEntry { concept, .. } => concept.as_ref(),
            _ => None,
        };
        if let Some(concept) = concept
            && concept.trim().is_empty()
        {
            return Err(NucleusError::Parse(format!(
                "`{}` needs a concept, and an empty one is not a concept",
                self.kind()
            )));
        }
        let named = match self {
            Self::InvokeCommand { command } | Self::RunCommand { command } => Some(command),
            Self::RunQuery { query, .. } => Some(query),
            Self::RunAction { action } => Some(action),
            _ => None,
        };
        if let Some(named) = named
            && named.trim().is_empty()
        {
            return Err(NucleusError::Parse(format!(
                "`{}` needs something to run",
                self.kind()
            )));
        }
        if let Self::Ask { options, .. } = self
            && options.iter().any(|option| option.trim().is_empty())
        {
            return Err(NucleusError::Parse(
                "a question cannot offer a blank answer".into(),
            ));
        }
        Ok(())
    }
}

impl Consequences {
    pub(in crate::karma) fn validate(items: &[Consequence]) -> Result<(), NucleusError> {
        if items.is_empty() {
            return Err(NucleusError::Parse(
                "a rule needs at least one consequence".into(),
            ));
        }
        items.iter().try_for_each(Consequence::validate)?;
        for (index, item) in items.iter().enumerate().skip(1) {
            if items.iter().take(index).any(|previous| previous == item) {
                return Err(NucleusError::Parse(format!(
                    "`{}` appears twice in one rule",
                    item.kind()
                )));
            }
        }
        Ok(())
    }
}
