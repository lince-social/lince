// non-lean-yet
// Scope: struct DraftFields, struct AssertionFields.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 60.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 60.

use super::{EngineError, invalid};
use nucleus::DecimalValue;
use serde_json::Value;

pub(crate) struct DraftFields<'a, A> {
    pub uid: &'a str,
    pub head: &'a str,
    pub body: &'a str,
    pub slug: Option<&'a str>,
    pub quantity: &'a str,
    pub assertions: A,
    pub work: &'a Value,
}

pub(crate) struct AssertionFields<'a> {
    pub predicate: &'a str,
    pub object: Option<&'a str>,
    pub quantity: Option<&'a str>,
    pub unit: Option<&'a str>,
}

impl<'a, A: ExactSizeIterator<Item = AssertionFields<'a>>> DraftFields<'a, A> {
    pub(crate) fn validate(mut self) -> Result<DecimalValue, EngineError> {
        let _validation = nucleus::performance::Span::new("record-draft-validation");
        nucleus::performance::add("record-draft-validations", 1);
        if !nucleus::valid_uid(self.uid, "r")
            || self.head.len() > 65536
            || self.body.len() > 262144
            || self.assertions.len() > 40
            || self.quantity.len() > 128
            || self
                .slug
                .is_some_and(|slug| slug.is_empty() || slug.len() > 128 || slug.trim() != slug)
            || self.assertions.any(|assertion| {
                if assertion.object.is_some_and(|value| {
                    value.is_empty()
                        || value.len()
                            > if lince_resources::reference::is_uri(value) {
                                8192
                            } else {
                                128
                            }
                }) {
                    return true;
                }
                [assertion.predicate]
                    .into_iter()
                    .chain(assertion.unit)
                    .chain(assertion.quantity)
                    .any(|value| value.is_empty() || value.len() > 128)
            })
        {
            return Err(invalid(
                "Check the Record fields and use at most 40 assertions",
            ));
        }
        crate::private_work::WorkMetadata::validate(self.work)
            .map_err(|error| invalid(error.to_string()))?;
        DecimalValue::parse_inferred(self.quantity)
            .map_err(|_| invalid("Enter an exact decimal quantity"))
    }
}
