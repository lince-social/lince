use lince_i18n::{Language, Token, catalog, initialize, native_text};

#[test]
fn startup_selection_freezes_the_catalog_and_resolves_ithkuil_fallbacks() {
    assert_eq!(catalog().language, Language::English);
    initialize(Language::Ithkuil).unwrap();
    let selected = catalog();
    assert_eq!(selected.language, Language::Ithkuil);
    assert_eq!(Token::Name.text(), "adnila");
    assert_eq!(Token::Save.text(), "Save");
    for token in [
        Token::Name,
        Token::Title,
        Token::Description,
        Token::Message,
        Token::Information,
        Token::PlainText,
    ] {
        let script = native_text(token.text()).unwrap();
        assert_ne!(script.encoding(), token.text());
        for _ in 0..10_000 {
            assert!(std::ptr::eq(script, native_text(token.text()).unwrap()));
        }
    }
    assert!(native_text(Token::Save.text()).is_none());
    initialize(Language::Ithkuil).unwrap();
    assert!(initialize(Language::PortugueseBrazil).is_err());
    assert!(std::ptr::eq(selected, catalog()));
}
