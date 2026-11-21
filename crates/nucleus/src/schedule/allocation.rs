// non-lean-yet
// Scope: struct Work, fn allocate.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 49, 54, 55, 57.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 67.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 12.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 8, 9, 12.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites allocate:
//   String (12).

use super::{MAX_INSTANT_MS, TimeRange};
use crate::DecimalValue;

#[derive(Clone, Debug)]
pub struct Work {
    pub id: String,
    pub release_ms: i64,
    pub duration_ms: i64,
    pub priority: Vec<Option<DecimalValue>>,
}

pub fn allocate(
    mut work: Vec<Work>,
    descending: &[bool],
) -> Result<Vec<(String, TimeRange)>, String> {
    let compare = |a: &Work, b: &Work| {
        for (index, descending) in descending.iter().enumerate() {
            let order = match (
                a.priority.get(index).copied().flatten(),
                b.priority.get(index).copied().flatten(),
            ) {
                (Some(a), Some(b)) => {
                    if *descending {
                        b.cmp(&a)
                    } else {
                        a.cmp(&b)
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if !order.is_eq() {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    };
    work.sort_by(|a, b| {
        a.release_ms
            .cmp(&b.release_ms)
            .then_with(|| compare(a, b))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut result = Vec::with_capacity(work.len());
    let mut cursor = 0;
    let mut index = 0;
    while index < work.len() {
        let first = &work[index];
        let start = cursor.max(first.release_ms);
        let mut end = start;
        let mut next = index;
        while next < work.len()
            && work[next].release_ms == first.release_ms
            && compare(first, &work[next]).is_eq()
        {
            let task = &work[next];
            if task.duration_ms < 0 || start < 0 {
                return Err("Invalid work allocation".into());
            }
            let until = start
                .checked_add(task.duration_ms)
                .filter(|until| *until <= MAX_INSTANT_MS)
                .ok_or("Work allocation overflow")?;
            end = end.max(until);
            result.push((
                task.id.clone(),
                TimeRange {
                    from_ms: start,
                    until_ms: (until > start).then_some(until),
                },
            ));
            next += 1;
        }
        cursor = end;
        index = next;
    }
    Ok(result)
}
