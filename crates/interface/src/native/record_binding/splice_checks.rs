use super::*;
use loro::ExportMode;

crate::native::laboratory_cases! {
    splice_preserves_unicode_mutations_and_cursors,
}

fn reference(doc: &LoroDoc, property: &str, before: &str, after: &str) {
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let text = doc.get_text(property);
    let removed = old.len() - prefix - suffix;
    if removed != 0 {
        text.delete(prefix, removed).unwrap();
    }
    let inserted: String = new[prefix..new.len() - suffix].iter().collect();
    if !inserted.is_empty() {
        text.insert(prefix, &inserted).unwrap();
    }
    doc.commit();
}

fn compare(property: &str, before: &str, after: &str) {
    let seed = LoroDoc::new();
    seed.set_peer_id(1).unwrap();
    seed.set_record_timestamp(false);
    let other = if property == "head" { "body" } else { "head" };
    seed.get_text(other).insert(0, "Untouched é").unwrap();
    seed.get_text(property).insert(0, before).unwrap();
    seed.commit();
    let version = seed.oplog_vv();
    let cursors: Vec<_> = (0..=before.chars().count())
        .filter_map(|index| {
            seed.get_text(property)
                .get_cursor(index, Default::default())
        })
        .collect();
    let snapshot = seed.export(ExportMode::Snapshot).unwrap();
    let actual = LoroDoc::new();
    let expected = LoroDoc::new();
    for doc in [&actual, &expected] {
        doc.import(&snapshot).unwrap();
        doc.set_peer_id(7).unwrap();
        doc.set_record_timestamp(false);
    }
    splice(&actual, property, before, after).unwrap();
    reference(&expected, property, before, after);
    assert_eq!(actual.get_text(property).to_string(), after);
    assert_eq!(actual.get_text(other).to_string(), "Untouched é");
    assert_eq!(actual.get_deep_value(), expected.get_deep_value());
    assert_eq!(actual.oplog_vv(), expected.oplog_vv());
    assert_eq!(actual.get_pending_txn_len(), 0);
    assert_eq!(
        serde_json::to_value(
            actual.export_json_updates_without_peer_compression(&version, &actual.oplog_vv())
        )
        .unwrap(),
        serde_json::to_value(
            expected.export_json_updates_without_peer_compression(&version, &expected.oplog_vv())
        )
        .unwrap()
    );
    for cursor in cursors {
        assert_eq!(
            actual.get_cursor_pos(&cursor).unwrap().current.pos,
            expected.get_cursor_pos(&cursor).unwrap().current.pos
        );
    }
    let reopened = LoroDoc::new();
    reopened
        .import(&actual.export(ExportMode::Snapshot).unwrap())
        .unwrap();
    assert_eq!(reopened.get_deep_value(), actual.get_deep_value());
}

#[cfg_attr(test, test)]
fn splice_preserves_unicode_mutations_and_cursors() {
    let texts = [
        "",
        "a",
        "ab",
        "é",
        "ê",
        "ĩ",
        "e\u{301}",
        "😀",
        "😁",
        "👩‍💻",
        "עברית",
        "東京",
        "\0",
        "a\nb",
        "\r\n",
        "aaaa",
    ];
    for property in ["head", "body"] {
        for before in texts {
            for after in texts {
                compare(property, before, after);
            }
        }
        for (before, after) in [
            ("αéZ", "αêZ"),
            ("αéZ", "αĩZ"),
            ("α👩‍💻Z", "α😀Z"),
            ("abc", "xabc"),
            ("abc", "abcx"),
            ("abcd", "abx"),
            ("banana", "bananana"),
            ("xaaaa", "aaaaa"),
        ] {
            compare(property, before, after);
        }
        let doc = LoroDoc::new();
        doc.get_text(property).insert(0, "same").unwrap();
        doc.commit();
        let other = if property == "head" { "body" } else { "head" };
        doc.get_text(other).insert(0, "pending").unwrap();
        assert!(doc.get_pending_txn_len() > 0);
        splice(&doc, property, "same", "same").unwrap();
        assert_eq!(doc.get_pending_txn_len(), 0);
        assert_eq!(doc.get_text(other).to_string(), "pending");
    }
}
