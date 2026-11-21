// non-lean-yet
// Scope: const UID, const PATH, const SOURCE, fn
//   discovery_uses_record_locations_and_ignores_non_requirement_markdown.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 58, 78.

use std::path::Path;

use lingua::{format, instinct, parse, project, requirements::discover, set_record_runtime};

const UID: &str = "r_04PESBG43ASBNW30QDN8KQPARV";
const PATH: &str = "requirements.lingua";
const SOURCE: &str = r#"Protein (
    @protein: 0, // |> [hidden-rule] INTERNAL_SENTINEL
    #instinct,
    //	|> [second-hidden-rule] Also hidden.
    #presented
) { r_04PESBG43ASBNW30QDN8KQPARV
	|> [visible-rule] Preserve the captions.
| --- | --- |
   | ordinary table cell | |> [table-example] |
> |> [quoted-example] Example.
if used || ready {}
```text
|> [backtick-example] Example.
```
~~~text
|> [tilde-example] Example.
~~~
  |> [later-visible-rule] Preserve selection.
} r_04PESBG43ASBNW30QDN8KQPARV
"#;

#[test]
fn discovery_uses_record_locations_and_ignores_non_requirement_markdown() {
    let requirements = discover(Path::new(PATH), SOURCE).unwrap();
    assert_eq!(
        requirements
            .iter()
            .map(|rule| (rule.id.as_str(), rule.line, rule.hidden))
            .collect::<Vec<_>>(),
        [
            ("hidden-rule", 2, true),
            ("second-hidden-rule", 4, true),
            ("visible-rule", 7, false),
            ("later-visible-rule", 18, false),
        ]
    );
    assert!(requirements.iter().all(|rule| rule.record_uid == UID));

    let moved = format!("Other (0) {{\nAn earlier Record.\n}}\n{SOURCE}");
    let after = discover(Path::new("moved.lingua"), &moved).unwrap();
    for (before, after) in requirements.iter().zip(after) {
        assert_eq!(before.id, after.id);
        assert_eq!(before.record_uid, after.record_uid);
        assert_eq!(before.line + 3, after.line);
    }
}

#[test]
fn hidden_comments_survive_format_and_runtime_writeback_and_stay_out_of_instinct() {
    let document = parse(SOURCE).unwrap();
    let formatted = format(&document);
    let records = project(&document).unwrap().records;
    let projected = &records[0];
    let updated = set_record_runtime(
        &formatted,
        UID,
        "Renamed Protein",
        &projected.body,
        "1",
        &projected.assertions,
    )
    .unwrap();
    for source in [SOURCE, &formatted, &updated] {
        let declared = discover(Path::new(PATH), source).unwrap();
        assert_eq!(declared.iter().filter(|rule| rule.hidden).count(), 2);
        assert!(source.contains("INTERNAL_SENTINEL"));
        let payload = instinct::validate(source, &[]).unwrap();
        assert!(
            !serde_json::to_string(&payload)
                .unwrap()
                .contains("INTERNAL_SENTINEL")
        );
        assert!(payload[0].body.contains("|> [visible-rule]"));
    }
}

#[test]
fn invalid_or_duplicate_rule_id_cannot_disappear_from_coverage() {
    let missing = SOURCE.replace("[visible-rule]", "");
    let missing = discover(Path::new(PATH), &missing).unwrap_err();
    assert_eq!(missing.path.as_deref(), Some(Path::new(PATH)));
    assert!(missing.message.contains("line 7"));

    let duplicate = SOURCE.replace("[visible-rule]", "[hidden-rule]");
    let duplicate = discover(Path::new(PATH), &duplicate).unwrap_err();
    assert!(
        duplicate
            .message
            .contains("Duplicate requirement [hidden-rule]")
    );
}
