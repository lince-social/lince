use utils::vault::{self, VaultError};

const RECORD: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const OTHER: &str = "r_01BX5ZZKBKACTAV9WEVGEMMVRZ";

#[test]
fn a_locked_description_round_trips_and_never_looks_like_its_plaintext() {
    let locked = vault::lock(RECORD, "correct horse", "provider token 12345").unwrap();
    assert!(locked.starts_with("lince-vault.v1 "));
    assert!(vault::is_locked(&locked));
    assert!(!locked.contains("provider token"));
    assert_eq!(
        vault::unlock(RECORD, "correct horse", &locked).unwrap(),
        "provider token 12345"
    );
}

#[test]
fn every_lock_draws_a_fresh_salt_and_nonce() {
    let first = vault::lock(RECORD, "same", "same text").unwrap();
    let second = vault::lock(RECORD, "same", "same text").unwrap();
    assert_ne!(first, second);
    let salt_and_nonce = |envelope: &str| {
        let fields: Vec<String> = envelope.split(' ').map(str::to_string).collect();
        (fields[2].clone(), fields[3].clone())
    };
    let (first_salt, first_nonce) = salt_and_nonce(&first);
    let (second_salt, second_nonce) = salt_and_nonce(&second);
    assert_ne!(first_salt, second_salt);
    assert_ne!(first_nonce, second_nonce);
}

#[test]
fn wrong_password_tampering_and_a_moved_record_uid_fail_the_same_way() {
    let locked = vault::lock(RECORD, "correct horse", "provider token 12345").unwrap();
    assert_eq!(
        vault::unlock(RECORD, "wrong horse", &locked),
        Err(VaultError::Unopenable)
    );
    assert_eq!(
        vault::unlock(OTHER, "correct horse", &locked),
        Err(VaultError::Unopenable)
    );

    let mut fields: Vec<String> = locked.split(' ').map(str::to_string).collect();
    let mut ciphertext = fields[4].clone().into_bytes();
    let last = ciphertext.len() - 2;
    ciphertext[last] = if ciphertext[last] == b'A' { b'B' } else { b'A' };
    fields[4] = String::from_utf8(ciphertext).unwrap();
    let tampered = fields.join(" ");
    assert_eq!(
        vault::unlock(RECORD, "correct horse", &tampered),
        Err(VaultError::Unopenable)
    );
}

#[test]
fn ordinary_text_is_not_a_vault_and_is_told_apart_from_a_bad_password() {
    assert!(!vault::is_locked("an ordinary description"));
    assert!(!vault::is_locked("lince-vault.v1 not really an envelope"));
    assert_eq!(
        vault::unlock(RECORD, "any", "an ordinary description"),
        Err(VaultError::NotAVault)
    );
}
