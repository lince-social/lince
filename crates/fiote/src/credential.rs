use {
    serde::{Deserialize, Serialize},
    zeroize::Zeroize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialSource {
    Environment,
    VaultRecord,
}

impl CredentialSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::VaultRecord => "vault Record",
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
