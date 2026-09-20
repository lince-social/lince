use std::collections::BTreeMap;

use serde_json::json;

#[test]
fn extensions_preserve_nested_values_and_resolve_the_record() {
    let source = "Food (@food: 0) {\nFood description.\n}\nExtension @food \"nutrition\" {\n basis_g 100\n omitted_nutrient_value 0\n values { protein_g 2.6 missing null zero 0 }\n sources [\"one\" \"two\"]\n verified true\n active false\n tiny 1e-6\n}\n";
    let (source, _) = anicca::ensure_uids(source).unwrap();
    let document = anicca::parse(&source).unwrap();
    let mut projected = anicca::project(&document).unwrap();
    let uid = projected.records[0].uid.clone();
    anicca::resolve_project_references(
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
    assert_eq!(source, anicca::canonicalize(&source).unwrap());
    let rendered = anicca::extension::render("food", "nutrition", &extension.fields).unwrap();
    let round_trip = anicca::project(&anicca::parse(&rendered).unwrap()).unwrap();
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
        assert!(anicca::parse(source).is_err(), "{source}");
    }
    let deep = format!(
        "Extension @food \"nutrition\" {{ x {}0{} }}",
        "[".repeat(40),
        "]".repeat(40)
    );
    assert!(anicca::parse(&deep).is_err());
    let mut projected =
        anicca::project(&anicca::parse("Extension @missing \"nutrition\" { x 1 }").unwrap())
            .unwrap();
    assert!(anicca::resolve_project_references(&mut projected, &BTreeMap::new()).is_err());
}
