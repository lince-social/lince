//! Provider credentials and their provenance.

use std::{env::var, fmt};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Where a provider credential was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialSource {
    /// The process environment.
    Environment,
    /// The local credential store.
    #[serde(alias = "vault-record")]
    LocalStore,
}

impl CredentialSource {
    /// A display label for the source.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::LocalStore => "local credential storage",
        }
    }
}

/// A provider secret that is cleared on drop and redacted in debug output.
pub struct ProviderCredential {
    variable: String,
    secret: Zeroizing<String>,
    source: CredentialSource,
}

impl ProviderCredential {
    /// Takes ownership of a provider secret and its source metadata.
    pub fn new(
        variable: impl Into<String>,
        secret: impl Into<String>,
        source: CredentialSource,
    ) -> Self {
        Self {
            variable: variable.into(),
            secret: Zeroizing::new(secret.into()),
            source,
        }
    }

    /// Loads a nonempty Unicode credential from the process environment.
    pub fn from_environment(variable: impl Into<String>) -> Option<Self> {
        let variable = variable.into();
        let secret = var(&variable).ok()?;
        (!secret.is_empty()).then(|| Self::new(variable, secret, CredentialSource::Environment))
    }

    /// The credential's source.
    pub fn source(&self) -> CredentialSource {
        self.source
    }

    /// The variable name used to identify this credential.
    pub fn variable(&self) -> &str {
        &self.variable
    }

    pub(crate) fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for ProviderCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderCredential")
            .field("variable", &self.variable)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
