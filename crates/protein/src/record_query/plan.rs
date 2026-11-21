use std::collections::BTreeSet;

use crate::{AggregateOp, GroupBy, Order, Protein, Source};

use super::{
    QueryError, RecordField, RecordProjection,
    budget::{Budget, within},
    output::{SortField, SortKey},
    predicate::validate_shape,
};

pub(super) struct ValidatedQuery {
    pub(super) projection: RecordProjection,
    pub(super) order: Vec<SortKey>,
}

pub(super) fn validate_query(
    query: &Protein,
    budget: &mut Budget<'_>,
) -> Result<ValidatedQuery, QueryError> {
    if query.source != Source::Record {
        return Err(QueryError::Unsupported);
    }
    if query
        .limit
        .is_some_and(|limit| limit > budget.limits.results)
    {
        return Err(QueryError::LimitExceeded);
    }
    within(query.filter.len(), budget.limits.predicate_nodes)?;
    for predicate in &query.filter {
        validate_shape(predicate, budget, 0)?;
    }
    let projection = validate_projection(query, budget)?;
    within(query.order.len(), budget.limits.order_fields)?;
    let mut order = Vec::with_capacity(query.order.len());
    let mut seen = BTreeSet::new();
    for entry in &query.order {
        budget.step()?;
        let (field, descending) = match entry {
            Order::Asc(field) => (field, false),
            Order::Desc(field) => (field, true),
            Order::Link(_) => return Err(QueryError::Unsupported),
        };
        budget.text(field)?;
        let field = match field.as_str() {
            "head" => SortField::Head,
            "slug" => SortField::Slug,
            "kind" => SortField::Kind,
            "quantity" => SortField::Quantity,
            "start_date" => SortField::StartDate,
            "due_date" => SortField::DueDate,
            "created_at" => SortField::CreatedAt,
            "updated_at" => SortField::UpdatedAt,
            _ => return Err(QueryError::Unsupported),
        };
        if !seen.insert(field) {
            return Err(QueryError::InvalidInput);
        }
        order.push(SortKey { field, descending });
    }
    if let Some(aggregate) = &query.aggregate
        && (aggregate.op != AggregateOp::Count
            || !matches!(
                aggregate.by,
                GroupBy::Total | GroupBy::Kind | GroupBy::Concept
            )
            || query.fields.is_some()
            || query.include.extension.is_some()
            || query.include.numeric_extensions
            || !query.order.is_empty())
    {
        return Err(QueryError::Unsupported);
    }
    Ok(ValidatedQuery { projection, order })
}

fn validate_projection(
    query: &Protein,
    budget: &mut Budget<'_>,
) -> Result<RecordProjection, QueryError> {
    let include = &query.include;
    if include.shader_effect
        || !include.numeric_fields.is_empty()
        || include.relation_context.is_some()
        || include.record_after.is_some()
        || include.facts.is_some()
        || include.promises.is_some()
        || include.links.is_some()
        || include.threads.is_some()
        || include.availability
        || include.projection.is_some()
        || include.contact
        || include.conversations
        || include.reference_reads
    {
        return Err(QueryError::Unsupported);
    }
    let mut fields = match &query.fields {
        None => BTreeSet::from([
            RecordField::Slug,
            RecordField::Head,
            RecordField::Quantity,
            RecordField::Concept,
            RecordField::Unit,
            RecordField::Place,
            RecordField::Organ,
            RecordField::StartDate,
            RecordField::DueDate,
            RecordField::CreatedAt,
            RecordField::UpdatedAt,
            RecordField::Revision,
        ]),
        Some(requested) => {
            within(requested.len(), budget.limits.projection_fields)?;
            let mut fields = BTreeSet::new();
            for field in requested {
                budget.step()?;
                budget.text(field)?;
                let field = match field.as_str() {
                    "uid" => RecordField::Uid,
                    "kind" => RecordField::Kind,
                    "slug" => RecordField::Slug,
                    "head" => RecordField::Head,
                    "body" => RecordField::Body,
                    "quantity" => RecordField::Quantity,
                    "concept" => RecordField::Concept,
                    "unit" => RecordField::Unit,
                    "place" => RecordField::Place,
                    "organ" => RecordField::Organ,
                    "start_date" => RecordField::StartDate,
                    "due_date" => RecordField::DueDate,
                    "created_at" => RecordField::CreatedAt,
                    "updated_at" => RecordField::UpdatedAt,
                    "revision" => RecordField::Revision,
                    "assertions" => RecordField::Assertions,
                    _ => return Err(QueryError::Unsupported),
                };
                if !fields.insert(field) {
                    return Err(QueryError::InvalidInput);
                }
            }
            fields
        }
    };
    fields.extend([RecordField::Uid, RecordField::Kind]);
    let extension_namespace = include
        .extension
        .as_ref()
        .map(|extension| {
            budget.text(&extension.namespace)?;
            if extension.namespace.is_empty() {
                return Err(QueryError::InvalidInput);
            }
            Ok(extension.namespace.clone())
        })
        .transpose()?;
    Ok(RecordProjection {
        fields,
        extension_namespace,
    })
}
