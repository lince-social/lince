use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::build_info::SelfApplyBlock;

const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/lince-social/lince/releases?per_page=100";
const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/lince-social/lince/releases/download";

pub const MANIFEST_NAME: &str = "lince-update.json";
pub const SIGNATURE_NAME: &str = "lince-update.json.sig";

pub const UPDATE_SIGNING_PUBLIC_KEY_HEX: &str =
    "4810bed0038acf62810ecff3ed28fcc8b2b924563cd6471ee7ea77cbe0006d3f";

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateManifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub assets: Vec<ManifestAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAsset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Unstamped,
    UpToDate,
    Available,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub availability: Availability,
    pub version: String,
    pub revision: String,
    pub channel: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_sha256: Option<String>,
    pub can_self_apply: bool,
    pub self_apply_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Unsigned,
    BadSignature,
    MalformedManifest,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            UpdateError::Unsigned => "the update manifest has no signature",
            UpdateError::BadSignature => "the update manifest signature does not verify",
            UpdateError::MalformedManifest => "the update manifest could not be parsed",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for UpdateError {}

impl UpdateManifest {
    pub fn asset_for(&self, target: &str) -> Option<&ManifestAsset> {
        if target.is_empty() {
            return None;
        }
        let matches = |asset: &&ManifestAsset| {
            asset.target == target
                || (!asset.target.is_empty() && asset.target.contains(target))
                || asset.name.contains(target)
        };
        self.assets
            .iter()
            .find(|asset| matches(asset) && asset.kind == "desktop")
            .or_else(|| self.assets.iter().find(matches))
    }
}

pub fn releases_api() -> String {
    std::env::var("LINCE_UPDATE_RELEASES_URL").unwrap_or_else(|_| DEFAULT_RELEASES_API.to_string())
}

pub fn download_base() -> String {
    std::env::var("LINCE_UPDATE_DOWNLOAD_BASE")
        .unwrap_or_else(|_| DEFAULT_DOWNLOAD_BASE.to_string())
}

pub fn manifest_url(tag: &str) -> String {
    format!(
        "{}/{tag}/{MANIFEST_NAME}",
        download_base().trim_end_matches('/')
    )
}

pub fn signature_url(tag: &str) -> String {
    format!(
        "{}/{tag}/{SIGNATURE_NAME}",
        download_base().trim_end_matches('/')
    )
}

pub fn checks_disabled() -> bool {
    matches!(
        std::env::var("LINCE_UPDATE_CHECK").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    )
}

pub fn latest_rolling_tag(releases_json: &str) -> Option<String> {
    let releases: Vec<serde_json::Value> = serde_json::from_str(releases_json).ok()?;
    releases
        .iter()
        .filter_map(|release| release.get("tag_name").and_then(serde_json::Value::as_str))
        .find(|tag| tag.starts_with("rolling-"))
        .map(str::to_string)
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    let expected = expected_hex.trim().to_ascii_lowercase();
    !expected.is_empty() && sha256_hex(bytes) == expected
}

pub fn signing_key_configured() -> bool {
    UPDATE_SIGNING_PUBLIC_KEY_HEX
        .bytes()
        .any(|byte| byte != b'0')
}

pub fn verify_manifest_signature(manifest_bytes: &[u8], signature_hex: &str) -> bool {
    let Some(key_bytes) = decode_hex(UPDATE_SIGNING_PUBLIC_KEY_HEX) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Some(signature_bytes) = decode_hex(signature_hex) else {
        return false;
    };
    let Ok(signature_array): Result<[u8; 64], _> = signature_bytes.try_into() else {
        return false;
    };
    let signature = Signature::from_bytes(&signature_array);
    verifying_key
        .verify_strict(manifest_bytes, &signature)
        .is_ok()
}

pub fn parse_verified_manifest(
    manifest_bytes: &[u8],
    signature_hex: Option<&str>,
) -> Result<UpdateManifest, UpdateError> {
    let signature_hex = signature_hex
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(UpdateError::Unsigned)?;
    if !verify_manifest_signature(manifest_bytes, signature_hex) {
        return Err(UpdateError::BadSignature);
    }
    serde_json::from_slice(manifest_bytes).map_err(|_| UpdateError::MalformedManifest)
}

pub fn evaluate(
    manifest: &UpdateManifest,
    current_revision: &str,
    target: Option<&str>,
    block: SelfApplyBlock,
) -> UpdateStatus {
    let asset = target.and_then(|value| manifest.asset_for(value));
    let availability = if current_revision == "unknown" {
        Availability::Unstamped
    } else if !manifest.revision.is_empty() && manifest.revision != current_revision {
        Availability::Available
    } else {
        Availability::UpToDate
    };
    let can_self_apply =
        availability == Availability::Available && asset.is_some() && block == SelfApplyBlock::None;
    UpdateStatus {
        availability,
        version: manifest.version.clone(),
        revision: manifest.revision.clone(),
        channel: manifest.channel.clone(),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_url: asset.map(|asset| asset.url.clone()),
        asset_sha256: asset
            .filter(|asset| !asset.sha256.is_empty())
            .map(|asset| asset.sha256.clone()),
        can_self_apply,
        self_apply_note: block.message().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const MANIFEST: &[u8] = br#"{"version":"0.7.0","revision":"abcdef1234567890","channel":"rolling","assets":[{"name":"Lince_0.7.0_amd64.AppImage","url":"https://example/app.AppImage","sha256":"aa","target":"x86_64-unknown-linux-gnu","kind":"desktop"},{"name":"Lince_0.7.0_x64-setup.exe","url":"https://example/setup.exe","sha256":"bb","target":"x86_64-pc-windows-msvc","kind":"desktop"}]}"#;

    fn manifest() -> UpdateManifest {
        serde_json::from_slice(MANIFEST).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn picks_the_asset_for_the_running_target() {
        let manifest = manifest();
        assert_eq!(
            manifest.asset_for("x86_64-unknown-linux-gnu").unwrap().name,
            "Lince_0.7.0_amd64.AppImage"
        );
        assert!(manifest.asset_for("").is_none());
        assert!(manifest.asset_for("sparc-unknown-none").is_none());
    }

    #[test]
    fn availability_has_three_states() {
        assert_eq!(
            evaluate(
                &manifest(),
                "unknown",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::Unstamped
        );
        assert_eq!(
            evaluate(
                &manifest(),
                "abcdef1234567890",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::UpToDate
        );
        assert_eq!(
            evaluate(
                &manifest(),
                "0000000000000000",
                Some("x86_64-unknown-linux-gnu"),
                SelfApplyBlock::None
            )
            .availability,
            Availability::Available
        );
    }

    #[test]
    fn can_self_apply_needs_available_asset_and_no_block() {
        let ok = evaluate(
            &manifest(),
            "0000000000000000",
            Some("x86_64-unknown-linux-gnu"),
            SelfApplyBlock::None,
        );
        assert!(ok.can_self_apply);
        let nix = evaluate(
            &manifest(),
            "0000000000000000",
            Some("x86_64-unknown-linux-gnu"),
            SelfApplyBlock::NixStore,
        );
        assert!(!nix.can_self_apply);
        assert!(nix.self_apply_note.contains("nixos-rebuild"));
    }

    #[test]
    fn latest_rolling_tag_takes_the_first_rolling_entry() {
        let json = r#"[{"tag_name":"v0.7.0"},{"tag_name":"rolling-41"},{"tag_name":"rolling-40"}]"#;
        assert_eq!(latest_rolling_tag(json).as_deref(), Some("rolling-41"));
    }

    #[test]
    fn sha256_round_trips() {
        let digest = sha256_hex(b"lince");
        assert!(verify_sha256(b"lince", &digest));
        assert!(verify_sha256(b"lince", &digest.to_uppercase()));
        assert!(!verify_sha256(b"lynx", &digest));
        assert!(!verify_sha256(b"lince", ""));
    }

    #[test]
    fn signature_verifies_only_for_the_matching_key_and_bytes() {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let good_signature = hex(&signing_key.sign(MANIFEST).to_bytes());
        let public_key_hex = hex(signing_key.verifying_key().as_bytes());

        assert!(super::verify_with_key(
            &public_key_hex,
            MANIFEST,
            &good_signature
        ));
        assert!(!super::verify_with_key(
            &public_key_hex,
            b"tampered",
            &good_signature
        ));
        let other_key = SigningKey::from_bytes(&[9u8; 32]);
        let wrong_signature = hex(&other_key.sign(MANIFEST).to_bytes());
        assert!(!super::verify_with_key(
            &public_key_hex,
            MANIFEST,
            &wrong_signature
        ));
    }

    #[test]
    fn parse_verified_manifest_rejects_missing_and_bad_signatures() {
        assert_eq!(
            parse_verified_manifest(MANIFEST, None).unwrap_err(),
            UpdateError::Unsigned
        );
        assert_eq!(
            parse_verified_manifest(MANIFEST, Some("   ")).unwrap_err(),
            UpdateError::Unsigned
        );
        assert_eq!(
            parse_verified_manifest(MANIFEST, Some(&"ab".repeat(64))).unwrap_err(),
            UpdateError::BadSignature
        );
    }
}

#[cfg(test)]
fn verify_with_key(public_key_hex: &str, message: &[u8], signature_hex: &str) -> bool {
    let Some(key_bytes) = decode_hex(public_key_hex) else {
        return false;
    };
    let Ok(key_array): Result<[u8; 32], _> = key_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_array) else {
        return false;
    };
    let Some(signature_bytes) = decode_hex(signature_hex) else {
        return false;
    };
    let Ok(signature_array): Result<[u8; 64], _> = signature_bytes.try_into() else {
        return false;
    };
    verifying_key
        .verify_strict(message, &Signature::from_bytes(&signature_array))
        .is_ok()
}
