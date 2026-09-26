use sha2::{Digest, Sha256};

use str0m_proto::crypto::Sha256Provider;

#[derive(Debug)]
pub struct RustCryptoSha256Provider;

impl Sha256Provider for RustCryptoSha256Provider {
    fn sha256(&self, data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
}
