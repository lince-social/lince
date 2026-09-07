use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub bytes: usize,
    pub canonical_bytes: usize,
    pub depth: usize,
    pub nodes: usize,
    pub string_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 1024 * 1024,
            canonical_bytes: 1024 * 1024,
            depth: 32,
            nodes: 32768,
            string_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    LimitExceeded,
    InvalidJson,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Private JSON refused: {self:?}")
    }
}

impl std::error::Error for Error {}

pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Value, Error> {
    limits.validate()?;
    if bytes.len() > limits.bytes {
        return Err(Error::LimitExceeded);
    }
    let (syntax, numbers) = numeric_tokens(bytes, limits.nodes)?;
    let mut budget = JsonBudget {
        limits,
        nodes: 0,
        refusal: None,
        numbers: numbers.into_iter(),
    };
    let mut deserializer = serde_json::Deserializer::from_slice(&syntax);
    let value = JsonSeed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(&mut deserializer)
    .map_err(|_| budget.refusal.unwrap_or(Error::InvalidJson))?;
    deserializer.end().map_err(|_| Error::InvalidJson)?;
    if budget.numbers.next().is_some() {
        return Err(Error::InvalidJson);
    }
    Ok(value)
}

pub fn canonical_bytes(value: &Value, limits: &Limits) -> Result<Vec<u8>, Error> {
    limits.validate()?;
    validate_value(value, limits)?;
    canonical_output_bytes(value, limits.canonical_bytes)
}

pub(crate) fn canonical_normalized_envelope_output_bytes(
    value: &Value,
    maximum_bytes: usize,
) -> Result<Vec<u8>, Error> {
    validate_canonical_limit(maximum_bytes)?;
    canonical_output_bytes(value, maximum_bytes)
}

fn canonical_output_bytes(value: &Value, maximum_bytes: usize) -> Result<Vec<u8>, Error> {
    let mut output = BoundedOutput {
        bytes: Vec::new(),
        limit: maximum_bytes,
    };
    canonical_value(value, &mut output).map_err(|_| Error::LimitExceeded)?;
    Ok(output.bytes)
}

impl Limits {
    pub fn validate(&self) -> Result<(), Error> {
        let hard = Self::default();
        let pairs = [
            (self.bytes, hard.bytes),
            (self.canonical_bytes, hard.canonical_bytes),
            (self.depth, hard.depth),
            (self.nodes, hard.nodes),
            (self.string_bytes, hard.string_bytes),
        ];
        if pairs
            .iter()
            .any(|(value, maximum)| *value == 0 || value > maximum)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(())
    }
}

fn validate_canonical_limit(value: usize) -> Result<(), Error> {
    if value == 0 || value > Limits::default().canonical_bytes {
        return Err(Error::InvalidLimits);
    }
    Ok(())
}

fn validate_value(value: &Value, limits: &Limits) -> Result<(), Error> {
    let mut pending = vec![(value, 1usize)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes = nodes.checked_add(1).ok_or(Error::LimitExceeded)?;
        if depth > limits.depth || nodes > limits.nodes {
            return Err(Error::LimitExceeded);
        }
        match value {
            Value::String(value) => validate_string(value, limits)?,
            Value::Array(values) => {
                validate_children(values.len(), nodes, pending.len(), limits)?;
                pending.extend(values.iter().rev().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                validate_children(values.len(), nodes, pending.len(), limits)?;
                for key in values.keys() {
                    validate_string(key, limits)?;
                }
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
    Ok(())
}

fn validate_children(
    children: usize,
    visited: usize,
    pending: usize,
    limits: &Limits,
) -> Result<(), Error> {
    let accounted = visited.checked_add(pending).ok_or(Error::LimitExceeded)?;
    if children > limits.nodes.saturating_sub(accounted) {
        return Err(Error::LimitExceeded);
    }
    Ok(())
}

fn validate_string(value: &str, limits: &Limits) -> Result<(), Error> {
    if value.len() > limits.string_bytes {
        return Err(Error::LimitExceeded);
    }
    Ok(())
}

fn numeric_tokens(bytes: &[u8], limit: usize) -> Result<(Vec<u8>, Vec<Number>), Error> {
    let mut index = 0;
    let mut numbers = Vec::new();
    let mut syntax = bytes.to_vec();
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                while index < bytes.len()
                    && matches!(bytes[index], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    index += 1;
                }
                let token =
                    std::str::from_utf8(&bytes[start..index]).map_err(|_| Error::InvalidJson)?;
                number_syntax(token.as_bytes())?;
                if numbers.len() >= limit {
                    return Err(Error::LimitExceeded);
                }
                let number = if token.contains(['.', 'e', 'E']) {
                    Number::from_f64(token.parse::<f64>().map_err(|_| Error::InvalidJson)?)
                        .ok_or(Error::InvalidJson)?
                } else if token.starts_with('-') {
                    token
                        .parse::<i64>()
                        .map(Number::from)
                        .map_err(|_| Error::InvalidJson)?
                } else {
                    token
                        .parse::<u64>()
                        .map(Number::from)
                        .map_err(|_| Error::InvalidJson)?
                };
                numbers.push(number);
                syntax[start] = b'0';
                syntax[start + 1..index].fill(b' ');
            }
            _ => index += 1,
        }
    }
    Ok((syntax, numbers))
}

fn number_syntax(token: &[u8]) -> Result<(), Error> {
    if token.len() > 128 {
        return Err(Error::LimitExceeded);
    }
    let mut index = usize::from(token.first() == Some(&b'-'));
    match token.get(index) {
        Some(b'0') => index += 1,
        Some(b'1'..=b'9') => {
            index += 1;
            while token.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        _ => return Err(Error::InvalidJson),
    }
    if token.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while token.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return Err(Error::InvalidJson);
        }
    }
    if matches!(token.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(token.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while token.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return Err(Error::InvalidJson);
        }
    }
    if index != token.len() {
        return Err(Error::InvalidJson);
    }
    Ok(())
}

struct JsonBudget<'a> {
    limits: &'a Limits,
    nodes: usize,
    refusal: Option<Error>,
    numbers: std::vec::IntoIter<Number>,
}

struct JsonSeed<'a, 'b> {
    budget: &'a mut JsonBudget<'b>,
    depth: usize,
}

impl JsonBudget<'_> {
    fn refuse<E: de::Error>(&mut self, refusal: Error) -> E {
        self.refusal = Some(refusal);
        E::custom("private JSON refused")
    }

    fn string<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        if value.len() > self.limits.string_bytes {
            return Err(self.refuse(Error::LimitExceeded));
        }
        Ok(())
    }

    fn number<E: de::Error>(&mut self) -> Result<Value, E> {
        self.numbers
            .next()
            .map(Value::Number)
            .ok_or_else(|| self.refuse(Error::InvalidJson))
    }
}

impl<'de> DeserializeSeed<'de> for JsonSeed<'_, '_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        self.budget.nodes += 1;
        if self.depth > self.budget.limits.depth || self.budget.nodes > self.budget.limits.nodes {
            return Err(self.budget.refuse(Error::LimitExceeded));
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for JsonSeed<'_, '_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded private JSON")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Value, E> {
        self.budget.number()
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Value, E> {
        self.budget.number()
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Value, E> {
        self.budget.number()
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.budget.string(value)?;
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        self.budget.string(&value)?;
        Ok(Value::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(JsonSeed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            self.budget.string(&key)?;
            if values.contains_key(&key) {
                return Err(self.budget.refuse(Error::InvalidJson));
            }
            let value = map.next_value_seed(JsonSeed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("private canonical bytes exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn canonical_value(value: &Value, output: &mut BoundedOutput) -> io::Result<()> {
    match value {
        Value::Array(values) => {
            output.write_all(b"[")?;
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.write_all(b",")?;
                }
                canonical_value(value, output)?;
            }
            output.write_all(b"]")
        }
        Value::Object(values) => {
            output.write_all(b"{")?;
            for (index, (key, value)) in values
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .enumerate()
            {
                if index > 0 {
                    output.write_all(b",")?;
                }
                serde_json::to_writer(&mut *output, key)?;
                output.write_all(b":")?;
                canonical_value(value, output)?;
            }
            output.write_all(b"}")
        }
        _ => serde_json::to_writer(output, value).map_err(io::Error::other),
    }
}
