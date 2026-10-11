use nucleus::{
    DecimalValue,
    schedule::allocation::{Work, allocate},
};

fn work(id: &str, release: i64, duration: i64, values: &[Option<&str>]) -> Work {
    Work {
        id: id.into(),
        release_ms: release,
        duration_ms: duration,
        priority: values
            .iter()
            .map(|value| value.map(|value| DecimalValue::parse_inferred(value).unwrap()))
            .collect(),
    }
}

#[test]
fn priorities_tie_in_parallel_then_wait_for_the_longest_range() {
    let rows = allocate(
        vec![
            work("low", 100, 10, &[Some("2"), Some("0")]),
            work("long", 100, 30, &[Some("10"), Some("1")]),
            work("short", 100, 20, &[Some("10"), Some("1")]),
            work("next", 100, 5, &[Some("10"), Some("2")]),
            work("missing", 100, 1, &[None, None]),
            work("later", 110, 10, &[Some("100"), Some("0")]),
        ],
        &[true, false],
    )
    .unwrap();
    let starts: Vec<_> = rows
        .iter()
        .map(|(id, time)| (id.as_str(), time.from_ms))
        .collect();
    assert_eq!(
        starts,
        [
            ("long", 100),
            ("short", 100),
            ("next", 130),
            ("low", 135),
            ("missing", 145),
            ("later", 146)
        ]
    );
}

#[test]
fn randomized_distinct_priorities_never_overlap_and_completion_compacts_the_queue() {
    for seed in 0..64 {
        let mut rows: Vec<_> = (0..100)
            .map(|index| {
                let priority = ((index * 37 + seed) % 100).to_string();
                work(
                    &format!("{index}"),
                    1000,
                    (index * 13 + seed) % 100 + 1,
                    &[Some(&priority)],
                )
            })
            .collect();
        let allocated = allocate(rows.clone(), &[true]).unwrap();
        for pair in allocated.windows(2) {
            assert_eq!(pair[0].1.until_ms, Some(pair[1].1.from_ms));
        }
        let first = allocated[0].0.clone();
        let duration = allocated[0].1.until_ms.unwrap() - 1000;
        rows.retain(|row| row.id != first);
        let compacted = allocate(rows, &[true]).unwrap();
        for ((before_id, before), (after_id, after)) in allocated[1..].iter().zip(compacted) {
            assert_eq!(before_id, &after_id);
            assert_eq!(before.from_ms - duration, after.from_ms);
        }
    }
}

#[test]
fn allocation_rejects_negative_duration_and_overflow() {
    assert!(allocate(vec![work("negative", 0, -1, &[])], &[]).is_err());
    assert!(
        allocate(
            vec![work("overflow", nucleus::schedule::MAX_INSTANT_MS, 1, &[])],
            &[]
        )
        .is_err()
    );
}
