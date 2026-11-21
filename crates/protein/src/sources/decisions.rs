//! Local Decision reads.

use crate::{Protein, ProteinError};
use nucleus::performance::Span;
use serde_json::Value;
use store::{Store, misc::open_decision_rows};

pub(crate) async fn execute_decisions(
    store: &Store,
    protein: &Protein,
) -> Result<Vec<Value>, ProteinError> {
    let decisions = open_decision_rows(&store.pool).await?;
    let _publish = Span::new("protein-decision-publish");
    Ok(decisions
        .into_iter()
        .take(protein.limit.map_or(usize::MAX, |limit| limit.max(1)))
        .map(|decision| {
            Value::from_iter([
                ("uid", decision.record_uid.into()),
                ("subject", decision.subject_uid.into()),
                ("kind", decision.kind.into()),
                ("question", decision.question.into()),
                ("options", decision.options),
            ])
        })
        .collect())
}
