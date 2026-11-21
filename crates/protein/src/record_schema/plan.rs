use std::{borrow::Cow, slice::from_ref};

use crate::{Include, Order, Predicate, Protein};

use super::requests;

const AVAILABILITY_FIELDS: &[&str] = &[
    "actual",
    "reserved",
    "available",
    "surplus",
    "can_offer",
    "actual_exact",
    "reserved_exact",
    "surplus_exact",
    "can_offer_exact",
    "availability_complete",
    "availability_incomplete_count",
    "scenario_required",
];

fn include_if<T: Clone>(requested: bool, value: &Option<T>) -> Option<T> {
    requested.then(|| value.clone()).flatten()
}

pub(crate) fn selected_include(query: &Protein) -> Cow<'_, Include> {
    if query.fields.is_none() {
        return Cow::Borrowed(&query.include);
    }
    let include = &query.include;
    let wants = |key| requests(query, key);
    Cow::Owned(Include {
        relation_context: include_if(wants("relation_context"), &include.relation_context),
        record_after: include.record_after.clone(),
        facts: include_if(wants("facts"), &include.facts),
        promises: include_if(wants("promises"), &include.promises),
        links: include_if(wants("links"), &include.links),
        threads: include_if(wants("threads"), &include.threads),
        availability: include.availability && AVAILABILITY_FIELDS.iter().copied().any(wants),
        extension: include_if(wants("extension"), &include.extension),
        shader_effect: include.shader_effect && wants("shader_effect"),
        numeric_fields: if wants("numeric_values") {
            include.numeric_fields.clone()
        } else {
            Vec::new()
        },
        numeric_extensions: include.numeric_extensions && wants("numeric_extensions"),
        projection: include_if(wants("projected"), &include.projection),
        contact: include.contact && wants("contact"),
        conversations: include.conversations && wants("conversations"),
        reference_reads: include.reference_reads && wants("reference_reads"),
    })
}

pub(crate) fn predicates_require_body(predicates: &[Predicate]) -> bool {
    predicates.iter().any(|predicate| match predicate {
        Predicate::All(children) | Predicate::Any(children) => predicates_require_body(children),
        Predicate::Not(child) => predicates_require_body(from_ref(child.as_ref())),
        Predicate::QuantityLt(_)
        | Predicate::QuantityLte(_)
        | Predicate::QuantityGt(_)
        | Predicate::QuantityGte(_)
        | Predicate::QuantityEq(_)
        | Predicate::UidEq(_)
        | Predicate::KindEq(_)
        | Predicate::SlugEq(_)
        | Predicate::ConceptIn(_)
        | Predicate::Relation { .. }
        | Predicate::Under { .. }
        | Predicate::WorkDate { .. }
        | Predicate::Near { .. }
        | Predicate::OrganEq(_)
        | Predicate::OrganIn(_)
        | Predicate::FileOriginEq(_) => false,
        _ => true,
    })
}

fn field_requires_body(field: &str) -> bool {
    !matches!(
        field,
        "uid"
            | "kind"
            | "slug"
            | "head"
            | "title"
            | "quantity"
            | "concept"
            | "concept_name"
            | "unit"
            | "place"
            | "organ"
            | "created_at"
            | "updated_at"
            | "created_hlc"
            | "start_date"
            | "due_date"
            | "estimate_min"
            | "spent_seconds"
            | "running_since"
            | "work_logs"
            | "work_timer"
            | "assertions"
            | "assignees"
            | "assignee"
            | "assignee_name"
            | "facts"
            | "promises"
            | "links"
            | "threads"
            | "extension"
            | "numeric_extensions"
            | "numeric_values"
            | "shader_effect"
            | "relation_context"
            | "reference_reads"
            | "contact"
            | "conversations"
            | "projected"
    ) && !AVAILABILITY_FIELDS.contains(&field)
}

// Include data is loaded independently of the Record body.
pub(crate) fn requires_body(query: &Protein) -> bool {
    predicates_require_body(&query.filter)
        || query.aggregate.is_none()
            && (query
                .fields
                .as_ref()
                .is_none_or(|fields| fields.iter().any(|field| field_requires_body(field)))
                || query.order.iter().any(|order| match order {
                    Order::Link(_) => false,
                    Order::Asc(field) | Order::Desc(field) => {
                        !field.starts_with('#') && field_requires_body(field)
                    }
                }))
}
