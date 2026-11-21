//! Options that attach related data to query rows.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{relation_context, shader_inputs};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Include {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_context: Option<relation_context::Include>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_after: Option<String>,
    pub facts: Option<FactsInclude>,
    pub promises: Option<PromisesInclude>,
    pub links: Option<LinksInclude>,
    pub threads: Option<ThreadsInclude>,
    #[serde(default)]
    pub availability: bool,
    pub extension: Option<ExtensionInclude>,
    #[serde(default)]
    pub shader_effect: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numeric_fields: Vec<shader_inputs::Request>,
    #[serde(default)]
    pub numeric_extensions: bool,
    pub projection: Option<ProjectionInclude>,
    #[serde(default)]
    pub contact: bool,
    #[serde(default)]
    pub conversations: bool,
    #[serde(default)]
    pub reference_reads: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionInclude {
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectionInclude {
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactsInclude {
    #[serde(default = "default_fact_limit")]
    pub limit: i64,
}

fn default_fact_limit() -> i64 {
    10
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromisesInclude {
    #[serde(default)]
    pub state: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinksInclude {
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub direction: LinkDirection,
    #[serde(default)]
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkDirection {
    #[default]
    Both,
    Out,
    In,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThreadsInclude {
    #[serde(default = "default_messages_limit")]
    pub messages_limit: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub message_limits: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub message_before: BTreeMap<String, MessageCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageCursor {
    pub created_at: String,
    pub uid: String,
}

impl Default for ThreadsInclude {
    fn default() -> Self {
        Self {
            messages_limit: default_messages_limit(),
            message_limits: BTreeMap::new(),
            message_before: BTreeMap::new(),
        }
    }
}

fn default_messages_limit() -> usize {
    50
}
