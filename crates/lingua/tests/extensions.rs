// non-lean-yet
// Scope: fn extensions_preserve_nested_values_and_resolve_the_record, fn
//   extensions_reject_ambiguous_and_unbounded_data.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 11, 17, 20, 23, 24, 28.

use std::collections::BTreeMap;

use serde_json::json;

#[test]
fn extensions_preserve_nested_values_and_resolve_the_record() {
    let source = "Food (@food: 0) {\nFood description.\n}\nExtension @food \"nutrition\" {\n basis_g 100\n omitted_nutrient_value 0\n values { protein_g 2.6 missing null zero 0 }\n sources [\"one\" \"two\"]\n verified true\n active false\n tiny 1e-6\n}\n";
    let (source, _) = lingua::ensure_uids(source).unwrap();
    let document = lingua::parse(&source).unwrap();
    let mut projected = lingua::project(&document).unwrap();
    let uid = projected.records[0].uid.clone();
    lingua::resolve_project_references(
        &mut projected,
        &BTreeMap::from([("food".into(), uid.clone())]),
    )
    .unwrap();
    let extension = &projected.extensions[0];
    assert_eq!(extension.target_uid.as_deref(), Some(uid.as_str()));
    assert_eq!(
        extension.fields["values"],
        json!({"protein_g": 2.6, "missing": null, "zero": 0})
    );
    assert_eq!(extension.fields["sources"], json!(["one", "two"]));
    assert_eq!(extension.fields["tiny"], json!(0.000001));
    assert_eq!(source, lingua::canonicalize(&source).unwrap());
    let rendered = lingua::extension::render("food", "nutrition", &extension.fields).unwrap();
    let round_trip = lingua::project(&lingua::parse(&rendered).unwrap()).unwrap();
    assert_eq!(extension.fields, round_trip.extensions[0].fields);
}

#[test]
fn extensions_reject_ambiguous_and_unbounded_data() {
    for source in [
        "Extension @food \"nutrition\" { protein_g 1 protein_g 2 }",
        "Extension @food \"nutrition\" {} Extension @food \"nutrition\" {}",
        "Extension @food \"\" {}",
        "Extension @food \"nutrition\" { bad 1e9999 }",
        "Extension @food \"nutrition\" { bad NaN }",
        r#"Extension @food "nutrition" { bad "\q" }"#,
        r#"Extension @food "nutrition" { bad "\uD800" }"#,
        r#"Extension @food "\uD800" {}"#,
    ] {
        assert!(lingua::parse(source).is_err(), "{source}");
    }
    let deep = format!(
        "Extension @food \"nutrition\" {{ x {}0{} }}",
        "[".repeat(40),
        "]".repeat(40)
    );
    assert!(lingua::parse(&deep).is_err());
    let mut projected =
        lingua::project(&lingua::parse("Extension @missing \"nutrition\" { x 1 }").unwrap())
            .unwrap();
    assert!(lingua::resolve_project_references(&mut projected, &BTreeMap::new()).is_err());
}
