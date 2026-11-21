use std::{
    cmp::Ordering::{Equal, Greater, Less},
    iter::{from_fn, once},
};

use super::{MAX_INSTANT_MS, TimeRange};
use crate::DecimalValue;

/// Work that can be placed at or after its release time.
#[derive(Clone, Debug)]
pub struct Work {
    /// Stable identifier used to order otherwise equivalent work.
    pub id: String,
    /// Earliest placement in Unix milliseconds.
    pub release_ms: i64,
    /// Time the work occupies, in milliseconds.
    pub duration_ms: i64,
    /// Priority keys, with missing values ordered last in either direction.
    pub priority: Vec<Option<DecimalValue>>,
}

/// Places work in release, priority, and identifier order.
/// Groups with identical release and priority start together. Each group's
/// longest duration determines when the next group can start.
///
/// # Errors
///
/// Returns the first invalid duration or out-of-range end time in placement order.
pub fn allocate(
    mut work: Vec<Work>,
    descending: &[bool],
) -> Result<Vec<(String, TimeRange)>, String> {
    let compare = |left: &[Option<DecimalValue>], right: &[Option<DecimalValue>]| {
        for (index, descending) in descending.iter().enumerate() {
            let order = match (
                left.get(index).copied().flatten(),
                right.get(index).copied().flatten(),
            ) {
                (Some(a), Some(b)) if *descending => b.cmp(&a),
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => Less,
                (None, Some(_)) => Greater,
                (None, None) => Equal,
            };
            if !order.is_eq() {
                return order;
            }
        }
        Equal
    };
    work.sort_by(|a, b| {
        a.release_ms
            .cmp(&b.release_ms)
            .then_with(|| compare(&a.priority, &b.priority))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut result = Vec::with_capacity(work.len());
    let mut work = work.into_iter().peekable();
    let mut cursor = 0;
    while let Some(Work {
        id,
        release_ms,
        duration_ms,
        priority,
    }) = work.next()
    {
        let start = cursor.max(release_ms);
        let mut end = start;
        let tied = from_fn(|| {
            work.next_if(|task| {
                task.release_ms == release_ms && compare(&priority, &task.priority).is_eq()
            })
        });
        for (id, duration_ms) in
            once((id, duration_ms)).chain(tied.map(|task| (task.id, task.duration_ms)))
        {
            if duration_ms < 0 {
                return Err("Invalid work allocation".into());
            }
            let until = start
                .checked_add(duration_ms)
                .filter(|until| *until <= MAX_INSTANT_MS)
                .ok_or("Work allocation overflow")?;
            end = end.max(until);
            result.push((
                id,
                TimeRange {
                    from_ms: start,
                    until_ms: (until > start).then_some(until),
                },
            ));
        }
        cursor = end;
    }
    Ok(result)
}
