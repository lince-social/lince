use std::{cmp::Ordering, mem::replace};

use nucleus::performance::{Span, add};

// Sorting positions keeps the Record payload stationary until its order is known.
pub(super) fn prepare<K>(
    keys: impl Iterator<Item = K>,
    compare: impl Fn(&K, &K) -> Ordering,
) -> Vec<usize> {
    let preparation = Span::new("protein-record-order-keys");
    let keys: Vec<_> = keys.collect();
    add(
        "protein-record-order-keys",
        u64::try_from(keys.len()).unwrap_or(u64::MAX),
    );
    drop(preparation);
    let _sorting = Span::new("protein-record-order-indices");
    if keys.is_sorted_by(|a, b| !compare(a, b).is_gt()) {
        return Vec::new();
    }
    let mut indices: Vec<_> = (0..keys.len()).collect();
    indices.sort_unstable_by(|&a, &b| compare(&keys[a], &keys[b]).then_with(|| a.cmp(&b)));
    indices
}

// A nonempty index vector is a full permutation. Consume each cycle by fixing
// its visited positions, leaving later visits to those positions as no-ops.
pub(super) fn apply<T>(rows: &mut [T], mut indices: Vec<usize>) {
    let _application = Span::new("protein-record-order-apply");
    let mut swaps = 0;
    for start in 0..indices.len() {
        let mut current = start;
        loop {
            let next = replace(&mut indices[current], current);
            if next == start {
                break;
            }
            rows.swap(current, next);
            swaps += 1;
            current = next;
        }
    }
    add("protein-record-order-payload-swaps", swaps);
}
