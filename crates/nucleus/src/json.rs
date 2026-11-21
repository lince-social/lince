// non-lean-yet
// Scope: fn same_value_encoding, fn same_values_encoding, fn encoded_len, fn sha256.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 38, 47.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   56.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 34, 42, 51.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 25,
//   34, 42, 51.

use std::io::{self, Write};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn same_value_encoding(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            left == right
                && left.is_f64() == right.is_f64()
                && (!left.is_f64()
                    || left.as_f64().map(f64::to_bits) == right.as_f64().map(f64::to_bits))
        }
        (Value::Array(left), Value::Array(right)) => same_values_encoding(left, right),
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|((key, left), (other, right))| {
                    key == other && same_value_encoding(left, right)
                })
        }
        _ => left == right,
    }
}

pub fn same_values_encoding(left: &[serde_json::Value], right: &[serde_json::Value]) -> bool {
    std::ptr::eq(left, right)
        || left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| same_value_encoding(left, right))
}

pub fn encoded_len<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<usize> {
    let _preparation = crate::performance::Span::new("json-size-preparation");
    let bytes = encode(value, |_| {})?;
    crate::performance::add("json-size-values-counted", 1);
    crate::performance::add("json-size-bytes-counted", bytes as u64);
    Ok(bytes)
}

pub fn sha256<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<[u8; 32]> {
    let _preparation = crate::performance::Span::new("json-hash-preparation");
    let mut digest = Sha256::new();
    let bytes = encode(value, |bytes| digest.update(bytes))?;
    crate::performance::add("json-values-hashed", 1);
    crate::performance::add("json-bytes-hashed", bytes as u64);
    Ok(digest.finalize().into())
}

pub fn sha256_hex<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<String> {
    use std::fmt::Write as _;
    let digest = sha256(value)?;
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(hex, "{byte:02x}").expect("writing into a String cannot fail");
    }
    Ok(hex)
}

fn encode<T: Serialize + ?Sized>(
    value: &T,
    accept: impl FnMut(&[u8]),
) -> serde_json::Result<usize> {
    let mut writer = Consumer { accept, bytes: 0 };
    serde_json::to_writer(&mut writer, value)?;
    Ok(writer.bytes)
}

struct Consumer<F> {
    accept: F,
    bytes: usize,
}

impl<F: FnMut(&[u8])> Write for Consumer<F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("JSON byte count overflow"))?;
        (self.accept)(bytes);
        self.bytes = total;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
