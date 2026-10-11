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
