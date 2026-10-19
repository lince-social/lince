use rustls::{
    crypto::WebPkiSupportedAlgorithms,
    pki_types::{AlgorithmIdentifier, InvalidSignature, SignatureVerificationAlgorithm, alg_id},
};
use std::sync::OnceLock;
mod cache;

pub fn cache_statistics() -> Option<(usize, u64, u64)> {
    cache::statistics()
}

#[derive(Debug)]
struct SdkEcdsa {
    public: AlgorithmIdentifier,
    signature: AlgorithmIdentifier,
    curve_bits: u32,
    digest_bits: u32,
}

impl SignatureVerificationAlgorithm for SdkEcdsa {
    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        self.public
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        self.signature
    }

    fn verify_signature(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), InvalidSignature> {
        if public_key.len() != 1 + self.curve_bits as usize / 4
            || public_key.first() != Some(&4)
            || message.len() > 1048576
            || signature.is_empty()
            || signature.len() > 104
        {
            return Err(InvalidSignature);
        }
        cache::verify(
            (self.curve_bits, self.digest_bits),
            public_key,
            message,
            signature,
            || self.verify_uncached(public_key, message, signature),
        )
    }
}

impl SdkEcdsa {
    fn verify_uncached(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), InvalidSignature> {
        #[cfg(target_os = "espidf")]
        {
            let started = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
            let result = unsafe {
                esp_idf_svc::sys::ailuros::lince_ecdsa_verify(
                    self.curve_bits,
                    self.digest_bits,
                    public_key.as_ptr(),
                    public_key.len(),
                    message.as_ptr(),
                    message.len(),
                    signature.as_ptr(),
                    signature.len(),
                )
            };
            let elapsed = unsafe { esp_idf_svc::sys::esp_timer_get_time() } - started;
            if elapsed >= 250_000 {
                crate::device::report(serde_json::json!({
                    "type":"ailuros-slow-tls-verification","curve_bits":self.curve_bits,
                    "elapsed_ms":elapsed / 1000,"valid":result == 0,
                }));
            }
            unsafe { esp_idf_svc::sys::vTaskDelay(1) };
            if result == 0 {
                Ok(())
            } else {
                Err(InvalidSignature)
            }
        }
        #[cfg(not(target_os = "espidf"))]
        {
            let expected_digest = if self.digest_bits == 256 {
                alg_id::ECDSA_SHA256
            } else {
                alg_id::ECDSA_SHA384
            };
            rustls_rustcrypto::provider()
                .signature_verification_algorithms
                .all
                .iter()
                .find(|algorithm| {
                    algorithm.public_key_alg_id() == self.public
                        && algorithm.signature_alg_id() == expected_digest
                })
                .ok_or(InvalidSignature)?
                .verify_signature(public_key, message, signature)
        }
    }
}

static ECDSA: [SdkEcdsa; 4] = [
    SdkEcdsa {
        public: alg_id::ECDSA_P256,
        signature: alg_id::ECDSA_SHA256,
        curve_bits: 256,
        digest_bits: 256,
    },
    SdkEcdsa {
        public: alg_id::ECDSA_P256,
        signature: alg_id::ECDSA_SHA384,
        curve_bits: 256,
        digest_bits: 384,
    },
    SdkEcdsa {
        public: alg_id::ECDSA_P384,
        signature: alg_id::ECDSA_SHA256,
        curve_bits: 384,
        digest_bits: 256,
    },
    SdkEcdsa {
        public: alg_id::ECDSA_P384,
        signature: alg_id::ECDSA_SHA384,
        curve_bits: 384,
        digest_bits: 384,
    },
];

pub fn algorithms(base: WebPkiSupportedAlgorithms) -> WebPkiSupportedAlgorithms {
    static ALGORITHMS: OnceLock<WebPkiSupportedAlgorithms> = OnceLock::new();
    *ALGORITHMS.get_or_init(|| {
        let replace = |algorithm: &'static dyn SignatureVerificationAlgorithm| {
            ECDSA
                .iter()
                .find(|replacement| {
                    replacement.public == algorithm.public_key_alg_id()
                        && replacement.signature == algorithm.signature_alg_id()
                })
                .map_or(algorithm, |value| {
                    value as &dyn SignatureVerificationAlgorithm
                })
        };
        let all = Box::leak(
            base.all
                .iter()
                .map(|algorithm| replace(*algorithm))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let mapping = Box::leak(
            base.mapping
                .iter()
                .map(|(scheme, algorithms)| {
                    let algorithms: &'static [_] = Box::leak(
                        algorithms
                            .iter()
                            .map(|algorithm| replace(*algorithm))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    );
                    (*scheme, algorithms)
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        WebPkiSupportedAlgorithms { all, mapping }
    })
}

pub fn qualify() -> anyhow::Result<()> {
    use anyhow::ensure;
    let vectors: serde_json::Value = serde_json::from_str(include_str!("../fixtures/ecdsa.json"))?;
    let decode = |value: &serde_json::Value| -> anyhow::Result<Vec<u8>> {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid signature vector"))?;
        ensure!(value.len() % 2 == 0, "Invalid signature vector");
        (0..value.len())
            .step_by(2)
            .map(|index| Ok(u8::from_str_radix(&value[index..index + 2], 16)?))
            .collect()
    };
    let vectors = vectors["vectors"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing signature vectors"))?;
    ensure!(vectors.len() == ECDSA.len(), "Missing signature vectors");
    for (algorithm, vector) in ECDSA.iter().zip(vectors) {
        let key = decode(&vector["public_key"])?;
        let signature = decode(&vector["signature"])?;
        let started = std::time::Instant::now();
        ensure!(
            algorithm
                .verify_signature(&key, b"sample", &signature)
                .is_ok(),
            "Valid signature rejected"
        );
        ensure!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "Signature verification exceeds the device deadline"
        );
        ensure!(
            algorithm
                .verify_signature(&key, b"tampered", &signature)
                .is_err(),
            "Invalid signature accepted"
        );
        ensure!(
            algorithm
                .verify_signature(&key, b"sample", &signature[..signature.len() - 1])
                .is_err(),
            "Truncated signature accepted"
        );
        let mut trailing = signature.clone();
        trailing.push(0);
        ensure!(
            algorithm
                .verify_signature(&key, b"sample", &trailing)
                .is_err(),
            "Trailing signature data accepted"
        );
        let mut invalid_key = vec![0; key.len()];
        invalid_key[0] = 4;
        ensure!(
            algorithm
                .verify_signature(&invalid_key, b"sample", &signature)
                .is_err(),
            "Invalid public key accepted"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rustls::pki_types::SignatureVerificationAlgorithm;

    #[test]
    fn rfc6979_vectors_reject_tampering_and_malformed_signatures() {
        super::qualify().unwrap();
    }

    #[test]
    fn replacement_preserves_certificate_and_handshake_algorithm_mapping() {
        let base = rusty_esp_iroh_host::crypto::provider().signature_verification_algorithms;
        let replaced = super::algorithms(base);
        assert_eq!(replaced.all.len(), base.all.len());
        assert_eq!(replaced.mapping.len(), base.mapping.len());
        for (before, after) in base.all.iter().zip(replaced.all) {
            assert_eq!(before.public_key_alg_id(), after.public_key_alg_id());
            assert_eq!(before.signature_alg_id(), after.signature_alg_id());
        }
        for ((before_scheme, before), (after_scheme, after)) in
            base.mapping.iter().zip(replaced.mapping)
        {
            assert_eq!(before_scheme, after_scheme);
            assert_eq!(before.len(), after.len());
            for (before, after) in before.iter().zip(*after) {
                assert_eq!(before.public_key_alg_id(), after.public_key_alg_id());
                assert_eq!(before.signature_alg_id(), after.signature_alg_id());
            }
        }
        for algorithm in &super::ECDSA {
            let selected = replaced
                .all
                .iter()
                .find(|candidate| {
                    candidate.public_key_alg_id() == algorithm.public_key_alg_id()
                        && candidate.signature_alg_id() == algorithm.signature_alg_id()
                })
                .unwrap();
            assert!(format!("{selected:?}").contains("SdkEcdsa"));
        }
    }
}
