#[test]
fn fenced_braces_preserve_the_body_and_following_records() {
    for fence in ["```", "~~~", "   ```", "   ~~~"] {
        let body = format!(
            "Before.\n{fence}bash\ncat <<EOF\nfoo.com {{\n    reverse_proxy 127.0.0.1:6174\n}}\nEOF\n{fence}\nAfter.\n"
        );
        let source = format!(
            "Installation (@installation: 0) {{\n{body}}}\nNext (@next: 0) {{\nNext body.\n}}\n"
        );
        let (identified, minted) = anicca::ensure_uids(&source).unwrap();
        assert_eq!(minted.len(), 2);
        let document = anicca::parse(&identified).unwrap();
        let projected = anicca::project(&document).unwrap();
        assert_eq!(projected.records.len(), 2);
        assert_eq!(projected.records[0].body, body);
        assert_eq!(projected.records[1].body, "Next body.\n");
        assert_eq!(anicca::canonicalize(&identified).unwrap(), identified);
        assert!(anicca::ensure_uids(&identified).unwrap().1.is_empty());
    }
}

#[test]
fn adjacent_records_with_fences_keep_their_own_boundaries() {
    for fence in ["```", "~~~", "   ```", "   ~~~"] {
        let body = format!("{fence}text\nAn example.\n{fence}\n");
        let source = format!("First (0) {{\n{body}}}\nSecond (0) {{\n{body}}}\n");
        let (identified, _) = anicca::ensure_uids(&source).unwrap();
        let projected = anicca::project(&anicca::parse(&identified).unwrap()).unwrap();
        assert_eq!(projected.records.len(), 2);
        assert_eq!(projected.records[0].body, body);
        assert_eq!(projected.records[1].body, body);
    }
}

#[test]
fn bundled_lince_document_can_receive_identities_and_be_projected() {
    let source = include_str!("../../../institute/anicca/Lince.lingua");
    let (identified, _) = anicca::ensure_uids(source).unwrap();
    let projected = anicca::project(&anicca::parse(&identified).unwrap()).unwrap();
    let installation = projected
        .records
        .iter()
        .find(|record| record.slug.as_deref() == Some("installation"))
        .unwrap();
    assert!(installation.body.contains("\n}\nEOF\n"));
    assert!(
        projected
            .records
            .iter()
            .any(|record| record.slug.as_deref() == Some("record"))
    );
}
