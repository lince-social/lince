// non-lean-yet
// Scope: fn urls_and_composition_imports_round_trip_without_loading_navigation, fn
//   import_statements_and_records_titled_import_are_distinct.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 9, 10, 12, 16, 18, 29.

#[test]
fn urls_and_composition_imports_round_trip_without_loading_navigation() {
    let source = "Import \"./parts/Definitions.lingua\"\nImport @r_00000000000000000000000002 from [\"./Parts.lingua\", \"https://example.org/Parts.lingua\"]\nPlato (@plato: 0, #part-of \"../Philosophers.lingua#r_00000000000000000000000003\") { r_00000000000000000000000001\n} r_00000000000000000000000001\n";
    let parsed = lingua::parse(source).unwrap();
    let formatted = lingua::format(&parsed);
    assert_eq!(lingua::parse(&formatted).unwrap(), parsed);
    let mut project = lingua::project(&parsed).unwrap();
    assert_eq!(project.imports.len(), 2);
    assert_eq!(project.imports[1].locations.len(), 2);
    assert_eq!(project.imports[1].target, "@r_00000000000000000000000002");
    assert_eq!(
        project.records[0].assertions[0].object_uri.as_deref(),
        Some("../Philosophers.lingua#r_00000000000000000000000003")
    );
    lingua::resolve_project_references(&mut project, &Default::default()).unwrap();
    assert!(project.records[0].body.is_empty());
    assert!(
        lingua::render_record(&project.records[0])
            .unwrap()
            .contains("#part-of \"../")
    );
}

#[test]
fn import_statements_and_records_titled_import_are_distinct() {
    let source = "Import \"./Part.lingua\"\nImport the prepared habit (@habit: 0) { r_00000000000000000000000001\n} r_00000000000000000000000001\n";
    let project = lingua::project(&lingua::parse(source).unwrap()).unwrap();
    assert_eq!(project.imports.len(), 1);
    assert_eq!(project.records[0].head, "Import the prepared habit");
}
