use std::time::{Duration, UNIX_EPOCH};

use super::peer::{Network, TurnServer};
use crate::{MediaError, Result};

impl Network {
    pub fn from_environment() -> Result<Self> {
        Self::from_values(|name| std::env::var(name).ok())
    }

    fn from_values(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let mut network = Self::default();
        if let Some(urls) = get("LINCE_CALL_STUN_URLS") {
            network.stun = urls
                .split(',')
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .map(str::to_owned)
                .collect();
        }
        network.relay_only = get("LINCE_CALL_RELAY_ONLY").as_deref() == Some("true");
        if let Some(urls) = get("LINCE_CALL_TURN_URLS") {
            let username = get("LINCE_CALL_TURN_USERNAME")
                .ok_or_else(|| MediaError("TURN username is missing".into()))?;
            let password = get("LINCE_CALL_TURN_PASSWORD")
                .ok_or_else(|| MediaError("TURN password is missing".into()))?;
            let expires = get("LINCE_CALL_TURN_EXPIRES")
                .and_then(|expires| expires.parse::<u64>().ok())
                .ok_or_else(|| {
                    MediaError("TURN credentials need an expiration time in Unix seconds".into())
                })?;
            let expires_at = UNIX_EPOCH
                .checked_add(Duration::from_secs(expires))
                .ok_or_else(|| MediaError("Invalid TURN expiration".into()))?;
            network.turn.push(TurnServer {
                urls: urls.split(',').map(str::trim).map(str::to_owned).collect(),
                username,
                password,
                expires_at,
            });
        }
        network.configuration()?;
        Ok(network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_credentials_require_expiration_and_errors_do_not_reveal_them() {
        let result = Network::from_values(|key| match key {
            "LINCE_CALL_TURN_URLS" => Some("turn:relay.example:3478".into()),
            "LINCE_CALL_TURN_USERNAME" => Some("user".into()),
            "LINCE_CALL_TURN_PASSWORD" => Some("secret-must-not-appear".into()),
            _ => None,
        });
        let error = result.err().unwrap().to_string();
        assert!(!error.contains("secret-must-not-appear"));
        assert!(
            Network::from_values(|key| (key == "LINCE_CALL_RELAY_ONLY").then(|| "true".into()))
                .is_err()
        );
    }
}
