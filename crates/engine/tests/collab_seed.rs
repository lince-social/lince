use std::collections::BTreeSet;

use engine::collab_guard::{AcceptedDoc, GuardError, Limits};
use loro::{ContainerID, ContainerType, ExportMode, JsonSchema, LoroDoc, VersionVector};
use protein::authority::Property;

fn restored(seed: &AcceptedDoc) -> LoroDoc {
    let doc = LoroDoc::new();
    let status = doc.import(seed.snapshot()).unwrap();
    assert!(status.pending.is_none_or(|pending| pending.is_empty()));
    assert_eq!(doc.get_pending_txn_len(), 0);
    assert!(!doc.is_detached());
    assert!(!doc.is_shallow());
    assert_eq!(doc.oplog_vv(), seed.version());
    doc
}

fn client(seed: &AcceptedDoc, offset: u64) -> LoroDoc {
    let doc = restored(seed);
    let peer = (offset..)
        .find(|peer| !seed.version().contains_key(peer))
        .unwrap();
    doc.set_peer_id(peer).unwrap();
    doc
}

fn delta(doc: &LoroDoc, base: &VersionVector) -> Vec<u8> {
    doc.commit();
    serde_json::to_vec(&doc.export_json_updates_without_peer_compression(base, &doc.oplog_vv()))
        .unwrap()
}

fn history(doc: &LoroDoc) -> JsonSchema {
    doc.export_json_updates_without_peer_compression(&VersionVector::default(), &doc.oplog_vv())
}

fn seed_error(head: &str, body: &str, limits: &Limits) -> GuardError {
    match AcceptedDoc::from_trusted_text(head, body, limits) {
        Ok(_) => panic!("seed unexpectedly accepted"),
        Err(error) => error,
    }
}

#[test]
fn collab_seed_empty_has_no_history_and_round_trips_accepted_version() {
    let limits = Limits {
        text_bytes: 0,
        history_atoms: 0,
        delta_atoms: 0,
        history_changes: 0,
        delta_changes: 0,
        peers: 1,
        ..Limits::default()
    };
    let seed = AcceptedDoc::from_trusted_text("", "", &limits).unwrap();
    assert_eq!(seed.head(), "");
    assert_eq!(seed.body(), "");
    assert_eq!(seed.version(), VersionVector::default());
    let doc = restored(&seed);
    assert_eq!(doc.len_ops(), 0);
    assert_eq!(doc.len_changes(), 0);
    assert!(history(&doc).changes.is_empty());
    let loaded = AcceptedDoc::from_trusted_snapshot(seed.snapshot(), &limits).unwrap();
    assert_eq!(loaded.version(), seed.version());
    assert_eq!(loaded.head(), seed.head());
    assert_eq!(loaded.body(), seed.body());
    assert_eq!(
        AcceptedDoc::empty(&limits).unwrap().version(),
        seed.version()
    );
}

#[test]
fn collab_seed_exact_unicode_and_single_field_inputs_are_not_normalized() {
    for (head, body) in [
        ("Title", ""),
        ("", "Body"),
        ("\0e\u{301} é 🧑🏽‍💻\r\n", "\t漢字\n😀\0é\r"),
        ("cid:root-secret:Map", "{\"head\":\"not metadata\"}"),
    ] {
        let seed = AcceptedDoc::from_trusted_text(head, body, &Limits::default()).unwrap();
        assert_eq!(seed.head().as_bytes(), head.as_bytes());
        assert_eq!(seed.body().as_bytes(), body.as_bytes());
        let doc = restored(&seed);
        assert_eq!(doc.get_text("head").to_string(), head);
        assert_eq!(doc.get_text("body").to_string(), body);
        assert_eq!(doc.len_ops(), head.chars().count() + body.chars().count());
        assert_eq!(doc.len_changes(), 1);
        assert_eq!(seed.version().len(), 1);
        let schema = history(&doc);
        let roots: BTreeSet<_> = schema
            .changes
            .iter()
            .flat_map(|change| &change.ops)
            .map(|op| match &op.container {
                ContainerID::Root {
                    name,
                    container_type: ContainerType::Text,
                } => name.to_string(),
                _ => panic!("seed created a non-root text operation"),
            })
            .collect();
        let expected: BTreeSet<_> = [("head", head), ("body", body)]
            .into_iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(root, _)| root.to_owned())
            .collect();
        assert_eq!(roots, expected);
        let loaded =
            AcceptedDoc::from_trusted_snapshot(seed.snapshot(), &Limits::default()).unwrap();
        assert_eq!(loaded.version(), seed.version());
        assert_eq!(loaded.head().as_bytes(), head.as_bytes());
        assert_eq!(loaded.body().as_bytes(), body.as_bytes());
        assert_eq!(restored(&loaded).len_ops(), doc.len_ops());
    }
}

#[test]
fn collab_seed_real_client_edits_both_roots_and_replays_normalized_delta() {
    let seed = AcceptedDoc::from_trusted_text("é😀", "a漢", &Limits::default()).unwrap();
    let version = seed.version();
    let snapshot = seed.snapshot().to_vec();
    let editor = client(&seed, 10);
    editor.get_text("head").delete(1, 1).unwrap();
    editor.get_text("head").insert(1, "Z").unwrap();
    editor.get_text("body").insert(2, "🦀").unwrap();
    let raw = delta(&editor, &version);
    let properties = BTreeSet::from([Property::Head, Property::Body]);
    let prepared = seed
        .prepare_delta(&raw, &properties, &Limits::default())
        .unwrap();
    assert_eq!(prepared.base_version(), &version);
    assert_eq!(prepared.touched_properties(), &properties);
    assert_eq!(prepared.head(), "éZ");
    assert_eq!(prepared.body(), "a漢🦀");
    assert!(!prepared.is_duplicate());
    let receiver = client(&seed, 20);
    let normalized: JsonSchema = serde_json::from_slice(prepared.normalized_delta()).unwrap();
    let status = receiver.import_json_updates(normalized).unwrap();
    assert!(status.pending.is_none_or(|pending| pending.is_empty()));
    assert_eq!(receiver.get_text("head").to_string(), prepared.head());
    assert_eq!(receiver.get_text("body").to_string(), prepared.body());
    let accepted = prepared.into_accepted_after_commit();
    assert_eq!(accepted.version(), receiver.oplog_vv());
    let loaded =
        AcceptedDoc::from_trusted_snapshot(accepted.snapshot(), &Limits::default()).unwrap();
    assert_eq!(loaded.version(), accepted.version());
    assert_eq!(loaded.head(), "éZ");
    assert_eq!(loaded.body(), "a漢🦀");
    assert_eq!(seed.version(), version);
    assert_eq!(seed.snapshot(), snapshot);
    assert_eq!(seed.head(), "é😀");
    assert_eq!(seed.body(), "a漢");
}

#[test]
fn collab_seed_real_concurrent_clients_converge_against_the_same_seed() {
    let seed = AcceptedDoc::from_trusted_text("T", "ab", &Limits::default()).unwrap();
    let alice = client(&seed, 100);
    let bob = client(&seed, 200);
    alice.get_text("body").insert(1, "😀").unwrap();
    bob.get_text("body").insert(1, "é").unwrap();
    let alice_delta = delta(&alice, &seed.version());
    let bob_delta = delta(&bob, &seed.version());
    let grant = BTreeSet::from([Property::Body]);
    let forward = seed
        .prepare_delta(&alice_delta, &grant, &Limits::default())
        .unwrap()
        .into_accepted_after_commit()
        .prepare_delta(&bob_delta, &grant, &Limits::default())
        .unwrap()
        .into_accepted_after_commit();
    let reverse = seed
        .prepare_delta(&bob_delta, &grant, &Limits::default())
        .unwrap()
        .into_accepted_after_commit()
        .prepare_delta(&alice_delta, &grant, &Limits::default())
        .unwrap()
        .into_accepted_after_commit();
    assert_eq!(forward.version(), reverse.version());
    assert_eq!(forward.body(), reverse.body());
    assert_eq!(forward.head(), "T");
    assert!(forward.body().contains('😀'));
    assert!(forward.body().contains('é'));
    assert_eq!(seed.body(), "ab");
}

#[test]
fn collab_seed_aggregate_utf8_bound_is_inclusive_and_precedes_history_work() {
    let head = "é😀";
    let body = "漢";
    let bytes = head.len() + body.len();
    let exact = Limits {
        text_bytes: bytes,
        ..Limits::default()
    };
    assert!(AcceptedDoc::from_trusted_text(head, body, &exact).is_ok());
    let too_small = Limits {
        text_bytes: bytes - 1,
        history_atoms: 0,
        delta_atoms: 0,
        ..Limits::default()
    };
    assert_eq!(
        seed_error(head, body, &too_small),
        GuardError::Limit("text bytes")
    );
    assert_eq!(
        seed_error(
            "a",
            "b",
            &Limits {
                text_bytes: 1,
                ..Limits::default()
            }
        ),
        GuardError::Limit("text bytes")
    );
}

#[test]
fn collab_seed_history_atoms_count_unicode_scalars_and_not_delta_admission() {
    let limits = Limits {
        history_atoms: 4,
        delta_atoms: 0,
        delta_changes: 0,
        delta_operations: 0,
        delta_bytes: 0,
        ..Limits::default()
    };
    let seed = AcceptedDoc::from_trusted_text("😀é", "e\u{301}", &limits).unwrap();
    assert_eq!(restored(&seed).len_ops(), 4);
    assert_eq!(
        seed_error("😀é", "e\u{301}x", &limits),
        GuardError::Limit("history atoms")
    );
    assert_eq!(
        seed_error(
            "ab",
            "cd",
            &Limits {
                history_atoms: 3,
                delta_atoms: 0,
                ..Limits::default()
            }
        ),
        GuardError::Limit("history atoms")
    );
}

#[test]
fn collab_seed_commits_one_change_and_refuses_a_zero_change_budget() {
    let limits = Limits {
        history_changes: 1,
        delta_changes: 0,
        ..Limits::default()
    };
    let seed = AcceptedDoc::from_trusted_text("head", "body", &limits).unwrap();
    let doc = restored(&seed);
    assert_eq!(doc.len_changes(), 1);
    assert_eq!(history(&doc).changes.len(), 1);
    let zero = Limits {
        history_changes: 0,
        ..limits
    };
    assert_eq!(
        seed_error("head", "body", &zero),
        GuardError::Limit("history changes")
    );
    assert!(AcceptedDoc::from_trusted_text("", "", &zero).is_ok());
}

#[test]
fn collab_seed_one_peer_budget_is_retained_when_loading_and_editing() {
    let limits = Limits {
        peers: 1,
        ..Limits::default()
    };
    let seed = AcceptedDoc::from_trusted_text("h", "b", &limits).unwrap();
    assert_eq!(seed.version().len(), 1);
    let loaded = AcceptedDoc::from_trusted_snapshot(seed.snapshot(), &limits).unwrap();
    assert_eq!(loaded.version(), seed.version());
    let editor = client(&seed, 1000);
    editor.get_text("body").insert(1, "x").unwrap();
    let raw = delta(&editor, &seed.version());
    let grant = BTreeSet::from([Property::Body]);
    assert!(matches!(
        seed.prepare_delta(&raw, &grant, &limits),
        Err(GuardError::Limit("peers"))
    ));
    assert!(
        seed.prepare_delta(&raw, &grant, &Limits { peers: 2, ..limits })
            .is_ok()
    );
    assert_eq!(seed.body(), "b");
    assert_eq!(seed.version(), loaded.version());
}

#[test]
fn collab_seed_history_byte_limits_cover_aggregate_text_and_json_expansion() {
    let limits = Limits {
        history_bytes: 8,
        delta_bytes: 0,
        ..Limits::default()
    };
    assert_eq!(
        seed_error("12345", "6789", &limits),
        GuardError::Limit("history bytes")
    );
    let escaped = "\0".repeat(128);
    let limits = Limits {
        history_bytes: 256,
        delta_bytes: 0,
        ..Limits::default()
    };
    assert_eq!(
        seed_error("", &escaped, &limits),
        GuardError::Limit("history bytes")
    );
    let seed = AcceptedDoc::from_trusted_text("h", &escaped, &Limits::default()).unwrap();
    let encoded = serde_json::to_vec(&history(&restored(&seed))).unwrap();
    assert!(encoded.len() > seed.head().len() + seed.body().len());
    let exact = Limits {
        history_bytes: encoded.len(),
        delta_bytes: 0,
        ..Limits::default()
    };
    assert!(AcceptedDoc::from_trusted_snapshot(seed.snapshot(), &exact).is_ok());
    assert!(matches!(
        AcceptedDoc::from_trusted_snapshot(
            seed.snapshot(),
            &Limits {
                history_bytes: encoded.len() - 1,
                ..exact
            }
        ),
        Err(GuardError::Limit("history bytes"))
    ));
}

#[test]
fn collab_seed_snapshot_output_limit_refuses_then_valid_seed_still_succeeds() {
    let too_small = Limits {
        snapshot_bytes: 1,
        ..Limits::default()
    };
    for (head, body) in [("", ""), ("title", "body")] {
        assert_eq!(
            seed_error(head, body, &too_small),
            GuardError::Limit("snapshot bytes")
        );
        let seed = AcceptedDoc::from_trusted_text(head, body, &Limits::default()).unwrap();
        let exact = Limits {
            snapshot_bytes: seed.snapshot().len(),
            ..Limits::default()
        };
        let loaded = AcceptedDoc::from_trusted_snapshot(seed.snapshot(), &exact).unwrap();
        assert_eq!(loaded.head(), head);
        assert_eq!(loaded.body(), body);
        assert_eq!(loaded.version(), seed.version());
        assert!(matches!(
            AcceptedDoc::from_trusted_snapshot(
                seed.snapshot(),
                &Limits {
                    snapshot_bytes: seed.snapshot().len() - 1,
                    ..exact
                }
            ),
            Err(GuardError::Limit("snapshot bytes"))
        ));
    }
}

#[test]
fn collab_seed_invalid_limits_refuse_even_empty_text_before_acceptance() {
    for limits in [
        Limits {
            peers: 0,
            ..Limits::default()
        },
        Limits {
            history_atoms: i32::MAX as usize + 1,
            ..Limits::default()
        },
        Limits {
            history_atoms: 1,
            delta_atoms: 2,
            ..Limits::default()
        },
        Limits {
            history_changes: 1,
            delta_changes: 2,
            ..Limits::default()
        },
        Limits {
            history_bytes: 1,
            delta_bytes: 2,
            ..Limits::default()
        },
        Limits {
            history_bytes: 0,
            delta_bytes: 0,
            ..Limits::default()
        },
        Limits {
            snapshot_bytes: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(seed_error("", "", &limits), GuardError::InvalidLimits);
        assert_eq!(
            seed_error("head", "body", &limits),
            GuardError::InvalidLimits
        );
    }
    assert!(AcceptedDoc::from_trusted_text("head", "body", &Limits::default()).is_ok());
}

#[test]
fn collab_seed_snapshot_import_refusals_never_replace_a_valid_seed() {
    let seed = AcceptedDoc::from_trusted_text("title", "body", &Limits::default()).unwrap();
    let version = seed.version();
    let snapshot = seed.snapshot().to_vec();
    for invalid in [&b"not a Loro snapshot"[..], &snapshot[..snapshot.len() / 2]] {
        assert!(matches!(
            AcceptedDoc::from_trusted_snapshot(invalid, &Limits::default()),
            Err(GuardError::ImportRefused)
        ));
    }
    let unsupported = LoroDoc::new();
    unsupported.get_text("secret").insert(0, "x").unwrap();
    unsupported.commit();
    assert!(matches!(
        AcceptedDoc::from_trusted_snapshot(
            &unsupported.export(ExportMode::Snapshot).unwrap(),
            &Limits::default()
        ),
        Err(GuardError::UnsupportedContainer)
    ));
    assert_eq!(seed.version(), version);
    assert_eq!(seed.snapshot(), snapshot);
    let editor = client(&seed, 10);
    editor.get_text("body").insert(4, "!").unwrap();
    let prepared = seed
        .prepare_delta(
            &delta(&editor, &version),
            &BTreeSet::from([Property::Body]),
            &Limits::default(),
        )
        .unwrap();
    assert_eq!(prepared.body(), "body!");
}

#[test]
fn collab_seed_is_not_a_write_grant_and_canceled_duplicate_footprints_remain() {
    let seed = AcceptedDoc::from_trusted_text("H", "B", &Limits::default()).unwrap();
    let editor = client(&seed, 10);
    editor.get_text("head").insert(1, "x").unwrap();
    editor.get_text("head").delete(1, 1).unwrap();
    let raw = delta(&editor, &seed.version());
    assert!(matches!(
        seed.prepare_delta(&raw, &BTreeSet::from([Property::Body]), &Limits::default()),
        Err(GuardError::Forbidden(Property::Head))
    ));
    let head_grant = BTreeSet::from([Property::Head]);
    let accepted = seed
        .prepare_delta(&raw, &head_grant, &Limits::default())
        .unwrap()
        .into_accepted_after_commit();
    assert_eq!(accepted.head(), "H");
    assert_ne!(accepted.version(), seed.version());
    let duplicate = accepted
        .prepare_delta(&raw, &head_grant, &Limits::default())
        .unwrap();
    assert!(duplicate.is_duplicate());
    assert_eq!(duplicate.touched_properties(), &head_grant);
    assert!(matches!(
        accepted.prepare_delta(&raw, &BTreeSet::new(), &Limits::default()),
        Err(GuardError::Forbidden(Property::Head))
    ));
}
