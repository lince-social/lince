use std::{
    env::{current_exe, var_os},
    process::Command,
};

use fiote::{CredentialSource, ProviderCredential};

#[test]
fn a_credential_says_where_it_came_from_and_never_prints_its_secret() {
    let credential = ProviderCredential::new(
        "LINCE_TEST_PROVIDER_KEY",
        "sk-not-a-real-token",
        CredentialSource::LocalStore,
    );
    assert_eq!(credential.source(), CredentialSource::LocalStore);
    assert_eq!(credential.source().as_str(), "local credential storage");
    assert_eq!(credential.variable(), "LINCE_TEST_PROVIDER_KEY");
    let printed = format!("{credential:?}");
    assert!(!printed.contains("sk-not-a-real-token"), "{printed}");
    assert!(printed.contains("LINCE_TEST_PROVIDER_KEY"));
}

#[test]
fn an_absent_or_empty_environment_variable_is_no_credential() {
    const CHILD: &str = "LINCE_TEST_PROVIDER_ENV_CHILD";
    if var_os(CHILD).is_none() {
        let executable = current_exe().expect("find the credential test binary");
        let output = Command::new(executable)
            .args([
                "--exact",
                "an_absent_or_empty_environment_variable_is_no_credential",
            ])
            .env(CHILD, "1")
            .env("LINCE_TEST_EMPTY_PROVIDER_KEY", "")
            .env_remove("LINCE_TEST_ABSENT_PROVIDER_KEY")
            .env("LINCE_TEST_PRESENT_PROVIDER_KEY", "value")
            .output()
            .expect("run the test with an isolated environment");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    assert!(ProviderCredential::from_environment("LINCE_TEST_EMPTY_PROVIDER_KEY").is_none());
    assert!(ProviderCredential::from_environment("LINCE_TEST_ABSENT_PROVIDER_KEY").is_none());
    let credential =
        ProviderCredential::from_environment("LINCE_TEST_PRESENT_PROVIDER_KEY").expect("present");
    assert_eq!(credential.source(), CredentialSource::Environment);
    assert_eq!(credential.source().as_str(), "environment");
}
