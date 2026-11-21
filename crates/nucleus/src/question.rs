use std::fmt::{Display, from_fn};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod answers;
mod choices;
mod schema;

pub use answers::validate_answers;
pub use choices::choices;
pub use schema::validate_schema;

#[cfg(test)]
mod tests;

const MAX_FORM_BYTES: usize = 64 * 1024;
const MAX_CHOICES: usize = 64;
const MAX_TEXT: usize = 8192;
const MAX_PATTERN_BYTES: usize = 64 * 1024;

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pending,
    Answered,
    Declined,
    Cancelled,
}

#[expect(missing_docs, reason = "Rustdoc feeds the published question schema")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub prompt: String,
    pub responder: String,
    pub schema: Value,
    pub state: State,
    pub answers: Option<Value>,
    pub expires_ms: Option<u64>,
}

impl Question {
    /// Checks the responder, form and permitted answer state.
    ///
    /// # Errors
    ///
    /// Returns the first prompt, responder, form or answer validation diagnostic.
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty()
            || self.prompt.len() > MAX_TEXT
            || self.responder.is_empty()
            || self.responder.len() > 256
        {
            return Err("A question needs a prompt and one intended responder.".into());
        }
        validate_schema(&self.schema)?;
        match (&self.state, &self.answers) {
            (State::Answered, Some(answers)) => validate_answers(&self.schema, answers),
            (State::Answered, None) => Err("An answered question needs its answers.".into()),
            (_, Some(_)) => Err("Only answered questions can contain answers.".into()),
            _ => Ok(()),
        }
    }

    /// Formats the question, including any answers, for display.
    pub fn text(&self) -> String {
        let Self {
            prompt,
            responder,
            schema,
            state,
            answers,
            ..
        } = self;
        let answers = from_fn(|output| match answers {
            Some(value) => Display::fmt(value, output),
            None => Ok(()),
        });
        format!("Question for {responder}: {prompt}\nState: {state:?}\nFields: {schema}\n{answers}")
    }

    /// Checks a proposed response against the original question and its responder.
    ///
    /// # Errors
    ///
    /// Returns the first transition, responder, expiry or answer validation diagnostic.
    pub fn response(&self, next: &Self, actor: &str, author: &str, now: u64) -> Result<(), String> {
        let Self {
            prompt,
            responder,
            schema,
            expires_ms,
            state: _,
            answers: _,
        } = self;
        if prompt != &next.prompt
            || responder != &next.responder
            || schema != &next.schema
            || expires_ms != &next.expires_ms
            || self.state != State::Pending
            || next.state == State::Pending
        {
            return Err("This question is no longer waiting, or its fields changed.".into());
        }
        if actor != self.responder && !(actor == author && next.state == State::Cancelled) {
            return Err("Only the intended responder can answer this question.".into());
        }
        if next.state != State::Cancelled && self.expires_ms.is_some_and(|expires| now >= expires) {
            return Err("This question expired. Ask for a new one.".into());
        }
        next.validate()
    }
}
