#[test]
fn urls_and_composition_imports_round_trip_without_loading_navigation() {
    let source = "Import \"./parts/Definitions.lingua\"\nImport @r_00000000000000000000000002 from [\"./Parts.lingua\", \"https://example.org/Parts.lingua\"]\nPlato (@plato: 0, #part-of \"../Philosophers.lingua#r_00000000000000000000000003\") { r_00000000000000000000000001\n} r_00000000000000000000000001\n";
    let parsed = anicca::parse(source).unwrap();
    let formatted = anicca::format(&parsed);
    assert_eq!(anicca::parse(&formatted).unwrap(), parsed);
    let mut project = anicca::project(&parsed).unwrap();
    assert_eq!(project.imports.len(), 2);
    assert_eq!(project.imports[1].locations.len(), 2);
    assert_eq!(project.imports[1].target, "@r_00000000000000000000000002");
    assert_eq!(
        project.records[0].assertions[0].object_uri.as_deref(),
        Some("../Philosophers.lingua#r_00000000000000000000000003")
    );
    anicca::resolve_project_references(&mut project, &Default::default()).unwrap();
    assert!(project.records[0].body.is_empty());
    assert!(
        anicca::render_record(&project.records[0])
            .unwrap()
            .contains("#part-of \"../")
    );
}

#[test]
fn import_statements_and_records_titled_import_are_distinct() {
    let source = "Import \"./Part.lingua\"\nImport the prepared habit (@habit: 0) { r_00000000000000000000000001\n} r_00000000000000000000000001\n";
    let project = anicca::project(&anicca::parse(source).unwrap()).unwrap();
    assert_eq!(project.imports.len(), 1);
    assert_eq!(project.records[0].head, "Import the prepared habit");
}
