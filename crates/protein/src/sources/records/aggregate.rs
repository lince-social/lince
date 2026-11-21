use std::collections::BTreeMap;

use nucleus::{
    DecimalValue,
    performance::{Span, add},
};
use serde_json::{Map, Value};
use store::{exact::zero, records::RecordRow};

use crate::{
    Aggregate, AggregateOp, GroupBy,
    sources::activity::{TOTAL, UNCLASSIFIED},
};

pub(super) fn aggregate_records<Body>(rows: &[RecordRow<Body>], agg: &Aggregate) -> Vec<Value> {
    let _aggregate = Span::new("protein-record-aggregate");
    let mut buckets: BTreeMap<(&str, &str), (DecimalValue, i64)> = BTreeMap::new();
    for r in rows {
        let key = match agg.by {
            GroupBy::Concept => r.identity_predicate_uid.as_deref().unwrap_or(UNCLASSIFIED),
            GroupBy::Kind => r.kind.as_str(),
            GroupBy::Total => TOTAL,
            GroupBy::CauseKind | GroupBy::Day | GroupBy::Month | GroupBy::Classification => "(n/a)",
        };
        let unit = r.unit_uid.as_deref().unwrap_or_default();
        let entry = buckets.entry((key, unit)).or_insert_with(|| (zero(), 0));
        entry.0 = entry.0.aligned_add(r.quantity).unwrap_or(entry.0);
        entry.1 += 1;
    }
    add("protein-record-aggregate-groups", buckets.len() as u64);
    buckets
        .into_iter()
        .map(|((group, unit), (total, count))| {
            let mut row = Map::from_iter([
                ("group".into(), group.into()),
                ("unit_uid".into(), (!unit.is_empty()).then_some(unit).into()),
            ]);
            if matches!(agg.op, AggregateOp::Sum) {
                row.insert("value".into(), total.to_string().into());
            }
            row.insert("count".into(), count.into());
            row.into()
        })
        .collect()
}
