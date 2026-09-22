#[test]
fn executable_revision_is_trimmed_and_cannot_be_replaced() {
    use utils::build_info::{revision, revision_is_stamped, set_revision, short_revision};

    assert_eq!(revision(), "unknown");
    assert!(!revision_is_stamped());
    set_revision(" \n");
    assert_eq!(revision(), "unknown");
    set_revision(" 0123456789abcdef \n");
    assert_eq!(revision(), "0123456789abcdef");
    assert!(revision_is_stamped());
    assert_eq!(short_revision(), "0123456789ab");
    set_revision("replacement");
    assert_eq!(revision(), "0123456789abcdef");
}
