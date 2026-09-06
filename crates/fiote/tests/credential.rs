use fiote::{CredentialSource, ProviderCredential};

#[test]
fn a_credential_says_where_it_came_from_and_never_prints_its_secret() {
    let credential = ProviderCredential::new(
        "ANTHROPIC_API_KEY",
        "sk-not-a-real-token",
        CredentialSource::VaultRecord,
    );
    assert_eq!(credential.source(), CredentialSource::VaultRecord);
    assert_eq!(credential.source().as_str(), "vault Record");
    assert_eq!(credential.variable(), "ANTHROPIC_API_KEY");
    let printed = format!("{credential:?}");
    assert!(!printed.contains("sk-not-a-real-token"), "{printed}");
    assert!(printed.contains("ANTHROPIC_API_KEY"));
}

#[test]
fn an_absent_or_empty_environment_variable_is_no_credential() {
    unsafe { std::env::set_var("LINCE_TEST_EMPTY_PROVIDER_KEY", "") };
    assert!(ProviderCredential::from_environment("LINCE_TEST_EMPTY_PROVIDER_KEY").is_none());
    assert!(ProviderCredential::from_environment("LINCE_TEST_ABSENT_PROVIDER_KEY").is_none());
    unsafe { std::env::set_var("LINCE_TEST_PRESENT_PROVIDER_KEY", "value") };
    let credential =
        ProviderCredential::from_environment("LINCE_TEST_PRESENT_PROVIDER_KEY").expect("present");
    assert_eq!(credential.source(), CredentialSource::Environment);
    assert_eq!(credential.source().as_str(), "environment");
}
