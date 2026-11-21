//! JSON encoding comparisons, byte counts, and digest helpers.

use std::{
    io::{self, Write},
    ptr,
};

use serde::Serialize;
use serde_json::{Result, Value, to_writer};
use sha2::{Digest, Sha256};

use crate::performance::{self, Span};

/// Compares encoded JSON values, including object order and floating-point representations.
pub fn same_value_encoding(left: &Value, right: &Value) -> bool {
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

/// Compares JSON values in their slice order.
pub fn same_values_encoding(left: &[Value], right: &[Value]) -> bool {
    ptr::eq(left, right)
        || left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| same_value_encoding(left, right))
}

/// Counts the bytes of the JSON encoding without storing the encoded output.
///
/// # Errors
///
/// Returns an error for failed serialization or a byte count outside the supported range.
pub fn encoded_len<T: Serialize + ?Sized>(value: &T) -> Result<usize> {
    let _preparation = Span::new("json-size-preparation");
    let bytes = encode(value, |_| {})?;
    performance::add("json-size-values-counted", 1);
    performance::add(
        "json-size-bytes-counted",
        u64::try_from(bytes).unwrap_or(u64::MAX),
    );
    Ok(bytes)
}

/// Hashes the bytes written by the JSON serializer in encoding order.
///
/// # Errors
///
/// Returns an error for failed serialization or a byte count outside the supported range.
pub fn sha256<T: Serialize + ?Sized>(value: &T) -> Result<[u8; 32]> {
    let _preparation = Span::new("json-hash-preparation");
    let mut digest = Sha256::new();
    let bytes = encode(value, |bytes| digest.update(bytes))?;
    performance::add("json-values-hashed", 1);
    performance::add(
        "json-bytes-hashed",
        u64::try_from(bytes).unwrap_or(u64::MAX),
    );
    Ok(digest.finalize().into())
}

/// Returns the JSON digest in lowercase hexadecimal.
///
/// # Errors
///
/// Returns the same errors as [sha256].
pub fn sha256_hex<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    sha256(value).map(|digest| lowercase_hex(&digest))
}

#[expect(
    clippy::indexing_slicing,
    reason = "each four-bit digit is within the 16-entry table"
)]
pub(crate) fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    encoded
}

fn encode<T: Serialize + ?Sized>(value: &T, accept: impl FnMut(&[u8])) -> Result<usize> {
    let mut writer = Consumer { accept, bytes: 0 };
    to_writer(&mut writer, value)?;
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
