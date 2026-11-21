//! Reader policy and the observable evidence that supports a claim.

use std::{cmp::Ordering, collections::BTreeSet};

use serde::Serialize;

use super::super::{
    IdentityContext,
    query::{Analysis, Query},
};
use super::{Category, Comparator, Metric, Statement};
use crate::{DecimalValue, karma::CanonicalHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportState {
    Unknown,
    PartialSupport,
    MatchesVisibleScope,
    AtLeastSupportedInVisibleScope,
    VisibleEvidenceExceedsClaim,
    ArithmeticUnavailable,
}

#[derive(Debug, Serialize)]
pub struct Support {
    pub state: SupportState,
    pub observed: Option<DecimalValue>,
    pub claimed: DecimalValue,
    pub partial: bool,
    pub observed_only: bool,
    pub policy: CanonicalHash,
}

/// Compare a current experience claim with evidence under its exact reporting policy.
///
/// # Errors
///
/// The statement is unsuitable for comparison or the analysis uses a different policy.
pub fn compare(
    statement: &Statement,
    query: &Query,
    analysis: &Analysis,
) -> Result<Support, String> {
    statement.validate()?;
    if statement.category != Category::Experience {
        return Err(
            "Credentials and recommendations are issuer assertions, not delivery totals".into(),
        );
    }
    if statement.withdrawn {
        return Err("A withdrawn claim cannot be checked as a current statement".into());
    }
    let expected = statement
        .descriptor
        .checking_query(query, statement.as_of)?;
    if expected != *query || analysis.policy != query.id()? {
        return Err("Check the claim's exact meaning, unit, role, stage and period under the reader's source policy".into());
    }
    let group = analysis.groups.iter().find(|group| {
        group.measurement == statement.descriptor.measurement
            && group.level == statement.descriptor.level
    });
    let observed = match statement.descriptor.metric {
        Metric::Quantity => group.and_then(|group| group.raw_quantity),
        Metric::ReportingOccurrences | Metric::PositiveOccurrences => group.map(|_| {
            let sources = analysis
                .rows
                .iter()
                .filter(|row| {
                    row.selected
                        && row.measurement == statement.descriptor.measurement
                        && row.level == statement.descriptor.level
                        && (statement.descriptor.metric != Metric::PositiveOccurrences
                            || row.quantity.is_positive())
                })
                .map(|row| &row.source)
                .collect::<BTreeSet<_>>();
            DecimalValue::from_integer(sources.len() as i128)
        }),
    };
    let state = match observed {
        None if group.is_some() => SupportState::ArithmeticUnavailable,
        None => SupportState::Unknown,
        Some(value) => match (
            value.exact_numeric_cmp(statement.amount),
            statement.comparator,
        ) {
            (Ordering::Less, _) => SupportState::PartialSupport,
            (Ordering::Equal, Comparator::Exact) => SupportState::MatchesVisibleScope,
            (Ordering::Greater, Comparator::Exact) => SupportState::VisibleEvidenceExceedsClaim,
            (_, Comparator::AtLeast) => SupportState::AtLeastSupportedInVisibleScope,
        },
    };
    Ok(Support {
        state,
        observed,
        claimed: statement.amount,
        partial: analysis.partial,
        observed_only: true,
        policy: analysis.policy.clone(),
    })
}

pub fn trusted_issuer_for_activity(
    statement: &Statement,
    sources: &Query,
    context: &IdentityContext,
    at: i64,
) -> Result<bool, String> {
    statement.validate()?;
    sources.validate()?;
    Ok(statement.category != Category::Experience
        && !statement.withdrawn
        && statement.issued_at <= at
        && at < statement.expires_at
        && sources.subject == statement.descriptor.subject
        && sources.role == statement.descriptor.role
        && sources
            .measurements
            .contains(&statement.descriptor.measurement)
        && context.readable
        && !context.blocked
        && (sources.rely_on_all_visible
            || sources.issuers.include.iter().any(|rule| {
                !rule.identities.is_empty()
                    || rule.known.is_some()
                    || rule.lower.is_some()
                    || rule.upper.is_some()
            }))
        && sources.issuers.allows(&statement.issuer, context)
        && !sources.exclude_everywhere.contains(&statement.issuer)
        && !sources
            .exclude_everywhere
            .contains(&statement.descriptor.subject))
}
