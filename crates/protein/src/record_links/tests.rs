use super::*;

fn scan_reference(
    rows: &[BinaryAssertionRow],
    record_uid: &str,
    links: &LinksInclude,
    concept_names: &HashMap<String, String>,
) -> Vec<Value> {
    let max_hops = links.depth.max(1);
    let mut frontier: HashSet<String> = HashSet::from([record_uid.to_string()]);
    let mut visited: HashSet<String> = frontier.clone();
    let mut seen_links: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for hop in 1..=max_hops {
        let mut next: HashSet<String> = HashSet::new();
        for link in rows {
            let outgoing = frontier.contains(&link.subject_uid);
            let incoming = frontier.contains(&link.object_uid);
            if !outgoing && !incoming {
                continue;
            }
            if matches!(links.direction, LinkDirection::Out) && !outgoing {
                continue;
            }
            if matches!(links.direction, LinkDirection::In) && !incoming {
                continue;
            }
            let other = if outgoing {
                link.object_uid.clone()
            } else {
                link.subject_uid.clone()
            };
            if !visited.contains(&other) {
                next.insert(other.clone());
            }
            if !seen_links.insert(link.uid.clone()) {
                continue;
            }
            out.push(json!({
                "uid": link.uid,
                "from": link.subject_uid,
                "to": link.object_uid,
                "kind_uid": link.predicate_uid,
                "kind": concept_names.get(&link.predicate_uid).unwrap_or(&link.predicate),
                "direction": if outgoing { "out" } else { "in" },
                "other": other,
                "hop": hop,
                "quantity": link.quantity.map(|value| value.to_f64()),
                "created_at": link.created_at,
            }));
        }
        if next.is_empty() {
            break;
        }
        visited.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

#[test]
fn indexed_traversal_matches_the_full_scan_for_order_direction_depth_cycles_and_parallel_edges() {
    let names = HashMap::from([("kind-0".into(), "Resolved meaning".into())]);
    for seed in 0..32u64 {
        let mut state = seed + 1;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 32) as usize
        };
        let mut edges = Vec::new();
        for index in 0..48 {
            edges.push(BinaryAssertionRow {
                uid: format!("edge-{index}"),
                subject_uid: format!("record-{}", next() % 12),
                object_uid: format!("record-{}", next() % 12),
                predicate_uid: format!("kind-{}", index % 2),
                predicate: "Fallback meaning".into(),
                quantity: (index % 2 == 0)
                    .then(|| nucleus::DecimalValue::parse_inferred("2.5").unwrap()),
                unit_uid: None,
                created_at: format!("2026-01-01T00:00:{index:02}Z"),
            });
        }
        edges.push(edges[0].clone());
        let graph = Graph::new(edges.clone());
        for direction in [LinkDirection::In, LinkDirection::Out, LinkDirection::Both] {
            for depth in [0, 1, 2, 5] {
                let settings = LinksInclude {
                    kinds: vec!["*".into()],
                    direction,
                    depth,
                };
                for record in 0..13 {
                    let record = format!("record-{record}");
                    assert_eq!(
                        graph.for_record(&record, &settings, &names),
                        scan_reference(&edges, &record, &settings, &names),
                        "seed={seed} root={record} direction={direction:?} depth={depth}"
                    );
                }
            }
        }
    }
    let graph = Graph::new(Vec::new());
    assert!(
        graph
            .for_record(
                "missing",
                &LinksInclude {
                    kinds: Vec::new(),
                    direction: LinkDirection::Both,
                    depth: 0
                },
                &names
            )
            .is_empty()
    );
}
