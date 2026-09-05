use chrono::Utc;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub fn ulid_from(millis: u64, entropy: u128) -> String {
    let value: u128 = ((millis as u128 & 0xFFFF_FFFF_FFFF) << 80) | (entropy & ((1u128 << 80) - 1));
    let mut out = String::with_capacity(26);
    for i in 0..26 {
        let shift = (25 - i) * 5;
        out.push(ALPHABET[((value >> shift) & 0x1F) as usize] as char);
    }
    out
}

pub fn new_uid(prefix: &str) -> String {
    let millis = Utc::now().timestamp_millis().max(0) as u64;
    let entropy = uuid::Uuid::new_v4().as_u128();
    format!("{prefix}_{}", ulid_from(millis, entropy))
}

pub fn valid_uid(uid: &str, prefix: &str) -> bool {
    let Some(body) = uid
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
    else {
        return false;
    };
    body.len() == 26 && body.bytes().all(|byte| ALPHABET.contains(&byte))
}

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
