//! Record property discovery, read planning, and metadata projection.

mod attachments;
mod plan;
mod work;

use serde::{Deserialize, Serialize};

use crate::Protein;

pub(crate) use attachments::attach;
pub(crate) use plan::{predicates_require_body, requires_body, selected_include};

/// Metadata for a Record property shown in the property picker.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Field {
    pub key: &'static str,
    pub title: &'static str,
    pub kind: &'static str,
    pub editable: bool,
}

impl Field {
    const fn editable(key: &'static str, title: &'static str, kind: &'static str) -> Self {
        Self {
            key,
            title,
            kind,
            editable: true,
        }
    }

    const fn read_only(key: &'static str, title: &'static str, kind: &'static str) -> Self {
        Self {
            editable: false,
            ..Self::editable(key, title, kind)
        }
    }
}

/// Record properties in the order used by the property picker.
pub const FIELDS: &[Field] = &[
    Field::editable("head", "Title", "text"),
    Field::editable("body", "Description", "text"),
    Field::editable("slug", "Slug", "text"),
    Field::editable("quantity", "Quantity", "decimal"),
    Field::editable("assertions", "Assertions", "assertions"),
    Field::read_only("kind", "Kind", "text"),
    Field::editable("assignees", "Assignees", "records"),
    Field::editable("start_date", "Start", "date-time"),
    Field::editable("due_date", "Due", "date-time"),
    Field::editable("estimate_min", "Estimate (minutes)", "number"),
    Field::read_only("spent_seconds", "Time spent (seconds)", "number"),
    Field::read_only("running_since", "Running since", "timestamp"),
    Field::editable("work_logs", "Work logs", "logs"),
    Field::editable("work_timer", "Work timer", "timer"),
    Field::editable("threads", "Threads and messages", "threads"),
    Field::read_only("created_at", "Created", "timestamp"),
    Field::read_only("updated_at", "Updated", "timestamp"),
];

/// Returns the property picker catalogue in display order.
pub fn fields() -> Vec<Field> {
    FIELDS.to_vec()
}

/// Returns the requested catalogue properties and the mandatory kind.
pub fn selected(protein: &Protein) -> Vec<Field> {
    FIELDS
        .iter()
        .copied()
        .filter(|field| field.key == "kind" || requests(protein, field.key))
        .collect()
}

pub(crate) fn requests(query: &Protein, key: &str) -> bool {
    query
        .fields
        .as_ref()
        .is_none_or(|fields| fields.iter().any(|field| field == key))
}
