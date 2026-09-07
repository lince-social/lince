use engine::private_password::{
    MAX_PASSWORD_BYTES, MAX_PHC_BYTES, MAX_WORKERS, PasswordError, PasswordHash, PasswordInput,
    PasswordWork,
};

const SALT: &str = "AAAAAAAAAAAAAAAAAAAAAA";
const OUTPUT: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn phc(algorithm: &str, version: &str, parameters: &str, salt: &str, output: &str) -> String {
    format!("${algorithm}${version}${parameters}${salt}${output}")
}

fn canonical() -> String {
    phc("argon2id", "v=19", "m=19456,t=2,p=1", SALT, OUTPUT)
}

fn input(bytes: &[u8]) -> PasswordInput {
    PasswordInput::new(bytes.to_vec()).unwrap()
}

fn rejects(encoded: String) {
    assert!(matches!(
        PasswordHash::from_phc(encoded),
        Err(PasswordError::InvalidHash)
    ));
}

#[test]
fn private_password_capacity_has_a_finite_nonzero_configuration() {
    for capacity in [0, MAX_WORKERS + 1, usize::MAX] {
        assert!(matches!(
            PasswordWork::new(capacity),
            Err(PasswordError::InvalidCapacity)
        ));
    }
    for capacity in 1..=MAX_WORKERS {
        assert!(PasswordWork::new(capacity).is_ok());
    }
}

#[test]
fn private_password_input_byte_bounds_are_exact() {
    assert!(matches!(
        PasswordInput::new(Vec::new()),
        Err(PasswordError::InvalidPassword)
    ));
    assert!(PasswordInput::new(vec![0]).is_ok());
    assert!(PasswordInput::new(vec![255; MAX_PASSWORD_BYTES]).is_ok());
    assert!(matches!(
        PasswordInput::new(vec![1; MAX_PASSWORD_BYTES + 1]),
        Err(PasswordError::InvalidPassword)
    ));
}

#[test]
fn private_password_input_bounds_count_utf8_bytes_not_characters() {
    assert!(PasswordInput::new("🦀".repeat(256).into_bytes()).is_ok());
    assert!(matches!(
        PasswordInput::new("🦀".repeat(257).into_bytes()),
        Err(PasswordError::InvalidPassword)
    ));
}

#[test]
fn private_password_canonical_hash_is_accepted_without_a_runtime() {
    let raw = canonical();
    assert!(raw.len() <= MAX_PHC_BYTES);
    assert_eq!(PasswordHash::from_phc(raw.clone()).unwrap().as_phc(), raw);
}

#[test]
fn private_password_phc_byte_cap_refuses_before_parsing_or_dispatch() {
    for length in [MAX_PHC_BYTES, MAX_PHC_BYTES + 1, MAX_PHC_BYTES * 4] {
        rejects("$".repeat(length));
    }
    rejects(format!("{}{}", canonical(), " ".repeat(MAX_PHC_BYTES)));
}

#[test]
fn private_password_algorithm_and_version_are_exact() {
    for algorithm in ["argon2i", "argon2d", "Argon2id", "argon2id ", "scrypt", ""] {
        rejects(phc(algorithm, "v=19", "m=19456,t=2,p=1", SALT, OUTPUT));
    }
    for version in ["v=16", "v=20", "v=019", "v=+19", "v=0x13", "19", ""] {
        rejects(phc("argon2id", version, "m=19456,t=2,p=1", SALT, OUTPUT));
    }
}

#[test]
fn private_password_costs_are_one_exact_profile_not_upper_bounds() {
    for parameters in [
        "m=19455,t=2,p=1",
        "m=19457,t=2,p=1",
        "m=8,t=1,p=1",
        "m=19456,t=1,p=1",
        "m=19456,t=3,p=1",
        "m=19456,t=2,p=2",
        "m=0,t=2,p=1",
        "m=19456,t=0,p=1",
        "m=19456,t=2,p=0",
        "m=4294967295,t=2,p=1",
        "m=19456,t=4294967295,p=1",
        "m=19456,t=2,p=4294967295",
        "m=18446744073709551616,t=2,p=1",
    ] {
        rejects(phc("argon2id", "v=19", parameters, SALT, OUTPUT));
    }
}

#[test]
fn private_password_duplicate_missing_extra_and_reordered_parameters_refuse() {
    for parameters in [
        "m=19456,m=19456,t=2,p=1",
        "m=4294967295,m=19456,t=2,p=1",
        "m=19456,m=4294967295,t=2,p=1",
        "m=19456,t=2,t=2,p=1",
        "m=19456,t=2,p=1,p=1",
        "m=19456,t=2",
        "m=19456,p=1",
        "t=2,p=1",
        "t=2,m=19456,p=1",
        "m=19456,p=1,t=2",
        "m=19456,t=2,p=1,keyid=YWJj",
        "m=19456,t=2,p=1,data=YWJj",
        "m=19456,t=2,p=1,x=1",
        "",
    ] {
        rejects(phc("argon2id", "v=19", parameters, SALT, OUTPUT));
    }
}

#[test]
fn private_password_alternate_parameter_number_spellings_refuse() {
    for parameters in [
        "m=019456,t=2,p=1",
        "m=19456,t=02,p=1",
        "m=19456,t=2,p=01",
        "m=+19456,t=2,p=1",
        "m=-19456,t=2,p=1",
        "m=19456.0,t=2,p=1",
        "m=1.9456e4,t=2,p=1",
        "m=19456,t=2,p=1 ",
        "m=19456, t=2,p=1",
        "m=19456,t=2,p=１",
    ] {
        rejects(phc("argon2id", "v=19", parameters, SALT, OUTPUT));
    }
}

#[test]
fn private_password_phc_shape_rejects_extra_sections_whitespace_and_controls() {
    for encoded in [
        String::new(),
        "plaintext".to_owned(),
        canonical().trim_start_matches('$').to_owned(),
        format!("${}", canonical()),
        format!("{}$", canonical()),
        format!("{}$extra", canonical()),
        format!(" {}", canonical()),
        format!("{}\n", canonical()),
        format!("{}\0", canonical()),
        format!("$argon2id$m=19456,t=2,p=1${SALT}${OUTPUT}"),
        format!("$argon2id$v=19$m=19456,t=2,p=1${SALT}"),
    ] {
        rejects(encoded);
    }
}

#[test]
fn private_password_salt_has_exact_decoded_size_and_canonical_base64() {
    for salt in [
        "",
        "AAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAAA",
        "AAAAAAAAAAAAAAAAAAAAAA==",
        "AAAAAAAAAAAAAAAAAAAAAB",
        "AAAAAAAAAAAAAAAAAAAAA_",
        "AAAAAAAAAAAAAAAAAAAAA-",
        "AAAAAAAAAAAAAAAAAAAAA ",
        "AAAAAAAAAAAAAAAAAAAAA$",
        "AAAAAAAAAAAAAAAAAAAAA\0",
        "AAAAAAAAAAAAAAAAAAAAé",
    ] {
        rejects(phc("argon2id", "v=19", "m=19456,t=2,p=1", salt, OUTPUT));
    }
}

#[test]
fn private_password_output_has_exact_decoded_size_and_canonical_base64() {
    for output in [
        String::new(),
        "A".repeat(42),
        "A".repeat(44),
        format!("{OUTPUT}="),
        format!("{}B", "A".repeat(42)),
        format!("{}_", "A".repeat(42)),
        format!("{}-", "A".repeat(42)),
        format!("{} ", "A".repeat(42)),
        format!("{}\0", "A".repeat(42)),
        format!("{}é", "A".repeat(41)),
    ] {
        rejects(phc("argon2id", "v=19", "m=19456,t=2,p=1", SALT, &output));
    }
}

#[test]
fn private_password_canonical_base64_supports_standard_plus_and_slash() {
    let salt = argon2::password_hash::SaltString::encode_b64(&[255; 16]).unwrap();
    let output = argon2::password_hash::Output::new(&[251; 32]).unwrap();
    let mut encoded = [0; 43];
    let output = output.b64_encode(&mut encoded).unwrap();
    let raw = phc("argon2id", "v=19", "m=19456,t=2,p=1", salt.as_str(), output);
    assert!(raw.contains('/'));
    assert!(raw.contains('+'));
    assert!(PasswordHash::from_phc(raw).is_ok());
}

#[test]
fn private_password_secret_types_and_errors_are_redacted() {
    let password = input(b"DISTINCT_SECRET_PASSWORD");
    let raw = canonical();
    let hash = PasswordHash::from_phc(raw.clone()).unwrap();
    assert_eq!(format!("{password:?}"), "PasswordInput([REDACTED])");
    assert_eq!(format!("{hash:?}"), "PasswordHash([REDACTED])");
    for error in [
        PasswordError::InvalidCapacity,
        PasswordError::InvalidPassword,
        PasswordError::InvalidHash,
        PasswordError::Busy,
        PasswordError::RuntimeUnavailable,
        PasswordError::RandomUnavailable,
        PasswordError::WorkerFailed,
    ] {
        for text in [error.to_string(), format!("{error:?}")] {
            assert!(!text.contains("DISTINCT_SECRET_PASSWORD"));
            assert!(!text.contains(&raw));
        }
        assert!(std::error::Error::source(&error).is_none());
    }
}

#[tokio::test]
async fn private_password_real_hash_matches_and_mismatch_is_not_a_worker_error() {
    let work = PasswordWork::new(1).unwrap();
    let hash = work
        .hash(input(b"correct horse battery staple"))
        .await
        .unwrap();
    let raw = hash.as_phc().to_owned();
    assert!(raw.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
    assert!(
        work.verify(input(b"correct horse battery staple"), hash)
            .await
            .unwrap()
    );
    assert!(
        !work
            .verify(input(b"incorrect"), PasswordHash::from_phc(raw).unwrap())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn private_password_new_hashes_use_fresh_sixteen_byte_salts() {
    let work = PasswordWork::new(1).unwrap();
    let first = work.hash(input(b"same password")).await.unwrap();
    let second = work.hash(input(b"same password")).await.unwrap();
    assert_ne!(first.as_phc(), second.as_phc());
    let first = argon2::PasswordHash::new(first.as_phc()).unwrap();
    let second = argon2::PasswordHash::new(second.as_phc()).unwrap();
    assert_ne!(first.salt, second.salt);
    for parsed in [first, second] {
        let mut salt = [0; 16];
        assert_eq!(
            parsed.salt.unwrap().decode_b64(&mut salt).unwrap().len(),
            16
        );
        assert_eq!(parsed.hash.unwrap().len(), 32);
    }
}

#[tokio::test]
async fn private_password_does_not_trim_spaces_or_line_endings() {
    let work = PasswordWork::new(1).unwrap();
    let hash = work.hash(input(b" password \n")).await.unwrap();
    let raw = hash.as_phc().to_owned();
    assert!(work.verify(input(b" password \n"), hash).await.unwrap());
    assert!(
        !work
            .verify(input(b"password"), PasswordHash::from_phc(raw).unwrap())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn private_password_handles_raw_nul_non_utf8_and_maximum_length() {
    let work = PasswordWork::new(1).unwrap();
    let bytes: Vec<u8> = (0..MAX_PASSWORD_BYTES).map(|index| index as u8).collect();
    let hash = work.hash(input(&bytes)).await.unwrap();
    assert!(work.verify(input(&bytes), hash).await.unwrap());
}

#[tokio::test]
async fn private_password_one_byte_and_whitespace_are_not_strength_claims() {
    let work = PasswordWork::new(1).unwrap();
    let hash = work.hash(input(b" ")).await.unwrap();
    assert!(work.verify(input(b" "), hash).await.unwrap());
}
