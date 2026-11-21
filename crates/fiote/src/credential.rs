// non-lean-yet
// Scope: enum CredentialSource, struct ProviderCredential.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 8.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 8, 9,
//   11, 15, 23, 30, 42, 51, 55.

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialSource {
    Environment,
    #[serde(alias = "vault-record")]
    LocalStore,
}

impl CredentialSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::LocalStore => "local credential storage",
        }
    }
}

pub struct ProviderCredential {
    variable: String,
    secret: String,
    source: CredentialSource,
}

impl ProviderCredential {
    pub fn new(
        variable: impl Into<String>,
        secret: impl Into<String>,
        source: CredentialSource,
    ) -> Self {
        Self {
            variable: variable.into(),
            secret: secret.into(),
            source,
        }
    }

    pub fn from_environment(variable: impl Into<String>) -> Option<Self> {
        let variable = variable.into();
        let secret = std::env::var(&variable).ok()?;
        if secret.is_empty() {
            return None;
        }
        Some(Self::new(variable, secret, CredentialSource::Environment))
    }

    pub fn source(&self) -> CredentialSource {
        self.source
    }

    pub fn variable(&self) -> &str {
        &self.variable
    }

    pub(crate) fn secret(&self) -> &str {
        &self.secret
    }
}

impl std::fmt::Debug for ProviderCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCredential")
            .field("variable", &self.variable)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl Drop for ProviderCredential {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}
