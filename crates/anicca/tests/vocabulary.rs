#[test]
fn unicode_and_quoted_concept_terms_round_trip_without_expanding_record_slugs() {
    let source = "Party (@party: 0, #tópos @street4, #\"saved geographic place\" @street4) { r_01JA0000000000000000000010\nParty details\n} r_01JA0000000000000000000010\n";
    let ast = anicca::parse(source).unwrap();
    let formatted = anicca::format(&ast);
    let projected = anicca::project(&ast).unwrap();
    assert_eq!(projected.records[0].assertions[0].predicate, "tópos");
    assert_eq!(
        projected.records[0].assertions[1].predicate,
        "saved geographic place"
    );
    let reparsed = anicca::project(&anicca::parse(&formatted).unwrap()).unwrap();
    assert_eq!(reparsed.records[0].assertions[0].predicate, "tópos");
    assert_eq!(
        reparsed.records[0].assertions[1].predicate,
        "saved geographic place"
    );
    assert!(anicca::parse(&source.replace("@party:", "@fête:")).is_err());
}
