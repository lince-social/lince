//! Query admission before any source is read.

use std::{error::Error, fmt};

use super::{DateComparison, Order, Predicate, Protein, Source, shader_inputs};
use chrono::{DateTime, NaiveDate};
use nucleus::{execution::now, sync::valid_file_origin};

const MAX_FILTER_INDENT: usize = 10;

/// A query operand or combination rejected before execution.
#[derive(Debug)]
pub struct ValidationError(pub(super) String);

impl ValidationError {
    /// Returns the reason without the protocol diagnostic prefix.
    pub fn into_reason(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Preserve the diagnostic presented to editors by the execution facade.
        write!(
            formatter,
            "encountered unexpected or invalid data: {}",
            self.0
        )
    }
}

impl Error for ValidationError {}

/// Checks which filters and includes the read executor can accept for a query.
///
/// # Errors
///
/// Returns a validation error for unsupported combinations or invalid operands.
pub fn validate(protein: &Protein) -> Result<(), ValidationError> {
    let Protein {
        source,
        include,
        aggregate,
        filter,
        order,
        ..
    } = protein;
    let record_rows = *source == Source::Record && aggregate.is_none();
    if !include.numeric_fields.is_empty() {
        if !record_rows {
            return Err(ValidationError(
                "Numeric inputs need a Record query without aggregation.".into(),
            ));
        }
        shader_inputs::validate(&include.numeric_fields)?;
    }
    if include.shader_effect && !record_rows {
        return Err(ValidationError(
            "Shader effects need a Record query without aggregation.".into(),
        ));
    }
    if include
        .relation_context
        .as_ref()
        .is_some_and(|context| !record_rows || !context.valid())
    {
        return Err(ValidationError("Invalid Relation color context".into()));
    }
    if include
        .extension
        .as_ref()
        .is_some_and(|extension| extension.namespace.starts_with("lince.social."))
    {
        return Err(ValidationError(
            "Private social state is available only through scoped social controls".into(),
        ));
    }

    for predicate in filter {
        validate_predicate(predicate, *source, 0)?;
    }
    if include
        .record_after
        .as_ref()
        .is_some_and(|after| !record_rows || after.is_empty() || after.len() > 256)
    {
        return Err(ValidationError(
            "Record paging needs a Record list and a valid cursor".into(),
        ));
    }
    if let Some(threads) = &include.threads
        && (threads.message_before.len() > 100
            || threads.message_before.values().any(|cursor| {
                cursor.uid.is_empty()
                    || cursor.uid.len() > 256
                    || cursor.created_at.len() > 64
                    || DateTime::parse_from_rfc3339(&cursor.created_at).is_err()
            }))
    {
        return Err(ValidationError("Invalid message history cursor".into()));
    }
    if *source == Source::Record
        && order
            .iter()
            .any(|order| matches!(order, Order::Asc(field) | Order::Desc(field) if field == "uid"))
    {
        return Err(ValidationError(
            "protein_record_uid_order_internal:uid is the implicit final Record tie-breaker".into(),
        ));
    }
    Ok(())
}

fn validate_predicate(
    predicate: &Predicate,
    source: Source,
    depth: usize,
) -> Result<(), ValidationError> {
    let top_level_window = depth == 0 && matches!(source, Source::Calendar | Source::Schedule);
    match predicate {
        Predicate::FileOriginEq(origin)
            if source != Source::Record || !valid_file_origin(origin) =>
        {
            Err(ValidationError(
                "File origin needs a Record query and a valid origin".into(),
            ))
        }
        Predicate::ProjectionWindow(window) => {
            if !top_level_window {
                return Err(ValidationError("protein_calendar_invalid:projection window must be a top-level Calendar or Schedule filter".into()));
            }
            window.validate().map_err(ValidationError)
        }
        Predicate::RollingWindow(window) => {
            if !top_level_window {
                return Err(ValidationError(
                    "Rolling windows must be top-level Calendar or Schedule filters".into(),
                ));
            }
            window
                .resolve(now().timestamp_millis())
                .map(|_| ())
                .map_err(ValidationError)
        }
        Predicate::All(children) | Predicate::Any(children) => {
            if depth > MAX_FILTER_INDENT {
                return Err(ValidationError(
                    "protein_filter_depth_exceeded:filter groups may be indented at most 10 levels"
                        .into(),
                ));
            }
            children
                .iter()
                .try_for_each(|child| validate_predicate(child, source, depth + 1))
        }
        Predicate::Not(child) => {
            if matches!(child.as_ref(), Predicate::All(_) | Predicate::Any(_)) {
                return Err(ValidationError(
                    "protein_filter_group_negation_unsupported:not may only wrap a condition"
                        .into(),
                ));
            }
            validate_predicate(child, source, depth + 1)
        }
        Predicate::WorkDate { op, value, .. }
            if *op != DateComparison::Exists
                && value
                    .as_deref()
                    .is_none_or(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()) =>
        {
            Err(ValidationError(
                "protein_work_date_invalid:work date must be YYYY-MM-DD".into(),
            ))
        }
        _ => Ok(()),
    }
}
