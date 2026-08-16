//! Prefixed ULIDs (`r_`, `f_`, `p_`, `l_`, `c_`, `t_` ...) and slug validation.
//! `ulid_from` is pure (blueprint 0.1: DST passes explicit time/entropy);
//! `new_uid` is the clocked convenience wrapper.

use chrono::Utc;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Crockford-base32 ULID from explicit parts: 48-bit millis + 80-bit entropy.
pub fn ulid_from(millis: u64, entropy: u128) -> String {
    let value: u128 = ((millis as u128 & 0xFFFF_FFFF_FFFF) << 80) | (entropy & ((1u128 << 80) - 1));
    let mut out = String::with_capacity(26);
    for i in 0..26 {
        let shift = (25 - i) * 5;
        out.push(ALPHABET[((value >> shift) & 0x1F) as usize] as char);
    }
    out
}

/// New uid with a type prefix, e.g. `new_uid("r")` -> `r_01J8...`.
pub fn new_uid(prefix: &str) -> String {
    let millis = Utc::now().timestamp_millis().max(0) as u64;
    let entropy = uuid::Uuid::new_v4().as_u128();
    format!("{prefix}_{}", ulid_from(millis, entropy))
}

/// Whether `uid` is a well-formed uid of `prefix` — `r_` plus 26 Crockford
/// base32 characters.
///
/// Exists because a uid may arrive from OUTSIDE this Cell: a `.lingua` file
/// written by hand can carry the uid of the Record it is going to become, so
/// that a folder of files can cross-link before any of them has been adopted.
/// Nothing downstream parses a uid, so a malformed one would not fail loudly —
/// it would simply be a Record whose identifier does not sort or compare like
/// any other, found much later.
pub fn valid_uid(uid: &str, prefix: &str) -> bool {
    let Some(body) = uid.strip_prefix(prefix).and_then(|rest| rest.strip_prefix('_')) else {
        return false;
    };
    body.len() == 26 && body.bytes().all(|byte| ALPHABET.contains(&byte))
}

/// Slug grammar: dot-separated segments of `[a-z0-9][a-z0-9-]*`.
pub fn valid_slug(slug: &str) -> bool {
    if slug.is_empty() {
        return false;
    }
    slug.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && seg
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
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
