use super::{EngineError, invalid};
use crate::private_work::WorkMetadata;
use lince_resources::reference::is_uri;
use nucleus::{
    DecimalValue,
    performance::{Span, add},
    valid_uid,
};
use serde_json::Value;

const MAX_HEAD_BYTES: usize = 65_536;
const MAX_BODY_BYTES: usize = 262_144;
const MAX_FIELD_BYTES: usize = 128;
const MAX_URI_BYTES: usize = 8_192;
const MAX_ASSERTIONS: usize = 40;

/// Borrowed draft fields shared by interactive and file-backed Record creation.
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
        let _validation = Span::new("record-draft-validation");
        add("record-draft-validations", 1);
        if !valid_uid(self.uid, "r")
            || self.head.len() > MAX_HEAD_BYTES
            || self.body.len() > MAX_BODY_BYTES
            || self.assertions.len() > MAX_ASSERTIONS
            || self.quantity.len() > MAX_FIELD_BYTES
            || self.slug.is_some_and(|slug| {
                slug.is_empty() || slug.len() > MAX_FIELD_BYTES || slug.trim() != slug
            })
            || !self.assertions.all(AssertionFields::valid)
        {
            return Err(invalid(
                "Check the Record fields and use at most 40 assertions",
            ));
        }
        WorkMetadata::validate(self.work).map_err(|error| invalid(error.to_string()))?;
        DecimalValue::parse_inferred(self.quantity)
            .map_err(|_| invalid("Enter an exact decimal quantity"))
    }
}

impl AssertionFields<'_> {
    fn valid(self) -> bool {
        self.object.is_none_or(|object| {
            !object.is_empty()
                && (object.len() <= MAX_FIELD_BYTES
                    || object.len() <= MAX_URI_BYTES && is_uri(object))
        }) && [self.predicate]
            .into_iter()
            .chain(self.unit)
            .chain(self.quantity)
            .all(|field| !field.is_empty() && field.len() <= MAX_FIELD_BYTES)
    }
}
