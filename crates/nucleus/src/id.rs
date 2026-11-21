//! Lince slug validation and prefixed, sortable identifiers.

pub use lince_karma::reference::valid_slug;

use crate::execution::{now, uuid};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Encodes the low 48 timestamp bits and low 80 entropy bits as a Crockford ULID.
pub fn ulid_from(millis: u64, entropy: u128) -> String {
    let value = (u128::from(millis) << 80) | (entropy & ((1 << 80) - 1));
    let mut encoded = String::with_capacity(26);
    for index in (0..26).rev() {
        let digit = ((value >> (index * 5)) & 31) as usize;
        encoded.push(char::from(ALPHABET[digit]));
    }
    encoded
}

/// Creates a prefixed ULID using the current execution context's clock and entropy.
pub fn new_uid(prefix: &str) -> String {
    let millis = u64::try_from(now().timestamp_millis()).unwrap_or_default();
    format!("{prefix}_{}", ulid_from(millis, uuid().as_u128()))
}

/// Checks a prefix, underscore, and 26 Crockford digits.
pub fn valid_uid(uid: &str, prefix: &str) -> bool {
    let Some(body) = uid
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
    else {
        return false;
    };
    body.len() == 26 && body.bytes().all(|byte| ALPHABET.contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_sortable_and_stable() {
        let a = ulid_from(1, 42);
        let b = ulid_from(2, 0);
        assert_eq!(a.len(), 26);
        assert!(a < b, "later millis must sort after");
        assert_eq!(ulid_from(1, 42), a, "pure: same inputs, same ulid");
        assert_eq!(ulid_from((1 << 48) + 1, (1 << 80) + 42), a);
    }

    #[test]
    fn uid_has_prefix() {
        let uid = new_uid("r");
        assert!(uid.starts_with("r_"));
        assert_eq!(uid.len(), 28);
    }

    #[test]
    fn slugs() {
        assert!(valid_slug("apples.stock"));
        assert!(valid_slug("rules.apple-reorder"));
        assert!(valid_slug("freq.daily-7am"));
        assert!(!valid_slug("Apples"));
        assert!(!valid_slug("a..b"));
        assert!(!valid_slug(".a"));
        assert!(!valid_slug(""));
    }
}
