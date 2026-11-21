// non-lean-yet
// Scope: fn fenced_braces_preserve_the_body_and_following_records, fn
//   adjacent_records_with_fences_keep_their_own_boundaries, fn
//   bundled_lince_document_can_receive_identities_and_be_projected.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 15, 16, 30, 31.

#[test]
fn fenced_braces_preserve_the_body_and_following_records() {
    for fence in ["```", "~~~", "   ```", "   ~~~"] {
        let body = format!(
            "Before.\n{fence}bash\ncat <<EOF\nfoo.com {{\n    reverse_proxy 127.0.0.1:6174\n}}\nEOF\n{fence}\nAfter.\n"
        );
        let source = format!(
            "Installation (@installation: 0) {{\n{body}}}\nNext (@next: 0) {{\nNext body.\n}}\n"
        );
        let (identified, minted) = lingua::ensure_uids(&source).unwrap();
        assert_eq!(minted.len(), 2);
        let document = lingua::parse(&identified).unwrap();
        let projected = lingua::project(&document).unwrap();
        assert_eq!(projected.records.len(), 2);
        assert_eq!(projected.records[0].body, body);
        assert_eq!(projected.records[1].body, "Next body.\n");
        assert_eq!(lingua::canonicalize(&identified).unwrap(), identified);
        assert!(lingua::ensure_uids(&identified).unwrap().1.is_empty());
    }
}

#[test]
fn adjacent_records_with_fences_keep_their_own_boundaries() {
    for fence in ["```", "~~~", "   ```", "   ~~~"] {
        let body = format!("{fence}text\nAn example.\n{fence}\n");
        let source = format!("First (0) {{\n{body}}}\nSecond (0) {{\n{body}}}\n");
        let (identified, _) = lingua::ensure_uids(&source).unwrap();
        let projected = lingua::project(&lingua::parse(&identified).unwrap()).unwrap();
        assert_eq!(projected.records.len(), 2);
        assert_eq!(projected.records[0].body, body);
        assert_eq!(projected.records[1].body, body);
    }
}

#[test]
fn bundled_lince_document_can_receive_identities_and_be_projected() {
    let source = include_str!("../../../institute/trails/Lince.lingua");
    let (identified, _) = lingua::ensure_uids(source).unwrap();
    let projected = lingua::project(&lingua::parse(&identified).unwrap()).unwrap();
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
