use std::io::{Read, Write};
use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

pub const JSON_BYTES: u64 = 16 * 1024 * 1024;
pub const LINES_BYTES: u64 = 1024 * 1024 * 1024;
pub const CAPTURE_BYTES: u64 = 1024 * 1024;
pub const BATCH_ROWS: usize = 256;
pub const MANIFEST_FILES: usize = 10 + 32 * (2 + 1 + 256);

pub fn manifest_files<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<std::collections::BTreeMap<String, nucleus::karma::CanonicalHash>, D::Error>
{
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = std::collections::BTreeMap<String, nucleus::karma::CanonicalHash>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("bounded manifest files")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut files = std::collections::BTreeMap::new();
            while let Some(path) = map.next_key::<String>()? {
                if files.len() >= MANIFEST_FILES || path.len() > 8192 || files.contains_key(&path) {
                    return Err(serde::de::Error::custom(
                        "manifest file count, path or duplicate exceeds its limit",
                    ));
                }
                files.insert(path, map.next_value()?);
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

#[derive(Debug)]
pub struct Exceeded {
    pub resource: &'static str,
    pub limit: u64,
}

impl std::fmt::Display for Exceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} exceeds its {} byte/row limit",
            self.resource, self.limit
        )
    }
}

impl std::error::Error for Exceeded {}

pub struct Writer<W> {
    inner: W,
    limit: u64,
    pub written: u64,
}

impl<W> Writer<W> {
    pub fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            limit,
            written: 0,
        }
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.limit.saturating_sub(self.written) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
                "simulation byte limit exceeded",
            ));
        }
        let count = self.inner.write(bytes)?;
        self.written += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn size<T: Serialize>(value: &T, limit: u64, resource: &'static str) -> crate::Result<u64> {
    let mut writer = Writer::new(std::io::sink(), limit);
    serde_json::to_writer(&mut writer, value).map_err(|error| -> crate::Error {
        if error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge) {
            Box::new(Exceeded { resource, limit })
        } else {
            Box::new(error)
        }
    })?;
    Ok(writer.written)
}

pub fn bytes<T: Serialize>(
    value: &T,
    limit: u64,
    resource: &'static str,
) -> crate::Result<Vec<u8>> {
    let size = size(value, limit, resource)?;
    let mut bytes = Vec::with_capacity(size as usize);
    serde_json::to_writer(&mut bytes, value)?;
    Ok(bytes)
}

pub fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> crate::Result<T> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(Box::new(Exceeded {
            resource: "JSON file",
            limit,
        }));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Box::new(Exceeded {
            resource: "JSON file",
            limit,
        }));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn read_array<T: DeserializeOwned>(
    path: &Path,
    limit: u64,
    row_limit: usize,
) -> crate::Result<Vec<T>> {
    use serde::de::{DeserializeSeed, SeqAccess, Visitor};
    struct Array<T>(usize, std::marker::PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> Visitor<'de> for Array<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded evidence array")
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Vec<T>, A::Error> {
            let mut rows = Vec::new();
            while rows.len() < self.0 {
                match sequence.next_element()? {
                    Some(row) => rows.push(row),
                    None => return Ok(rows),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "evidence array exceeds its row limit",
                ));
            }
            Ok(rows)
        }
    }
    impl<'de, T: serde::Deserialize<'de>> DeserializeSeed<'de> for Array<T> {
        type Value = Vec<T>;
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            deserializer.deserialize_seq(self)
        }
    }
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(Box::new(Exceeded {
            resource: "JSON array",
            limit,
        }));
    }
    let mut reader = std::io::BufReader::new(file.take(limit.saturating_add(1)));
    let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
    let rows = Array(row_limit, std::marker::PhantomData).deserialize(&mut deserializer)?;
    deserializer.end()?;
    if reader.get_ref().limit() == 0 {
        return Err(Box::new(Exceeded {
            resource: "JSON array",
            limit,
        }));
    }
    Ok(rows)
}

pub fn rows<T>(bytes: u64) -> u64 {
    bytes / std::mem::size_of::<T>().max(1) as u64
}

pub fn event_rows(scenario: &crate::scenario::Scenario) -> u64 {
    rows::<nucleus::simulation::Event>(scenario.limits.evidence_bytes).min(
        scenario.limits.steps.saturating_mul(256).saturating_add(
            scenario
                .cells
                .iter()
                .map(|cell| cell.seed.len() as u64 * 256 + 16)
                .sum::<u64>(),
        ),
    )
}

pub fn is_exceeded(error: &crate::Error) -> bool {
    error.downcast_ref::<Exceeded>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_deserialization_rejects_excess_and_duplicate_entries() {
        #[derive(serde::Deserialize)]
        struct Manifest {
            #[serde(deserialize_with = "manifest_files")]
            files: std::collections::BTreeMap<String, nucleus::karma::CanonicalHash>,
        }
        let hash = format!("sha256:{}", "0".repeat(64));
        let valid = format!("{{\"files\":{{\"result.json\":\"{hash}\"}}}}");
        assert_eq!(
            serde_json::from_str::<Manifest>(&valid)
                .unwrap()
                .files
                .len(),
            1
        );
        let duplicate =
            format!("{{\"files\":{{\"result.json\":\"{hash}\",\"result.json\":\"{hash}\"}}}}");
        assert!(
            serde_json::from_str::<Manifest>(&duplicate)
                .err()
                .unwrap()
                .to_string()
                .contains("duplicate")
        );
        let files: std::collections::BTreeMap<_, _> = (0..=MANIFEST_FILES)
            .map(|index| (format!("file{index}"), hash.clone()))
            .collect();
        let excessive = serde_json::to_string(&serde_json::json!({"files":files})).unwrap();
        assert!(
            serde_json::from_str::<Manifest>(&excessive)
                .err()
                .unwrap()
                .to_string()
                .contains("file count")
        );
    }

    #[test]
    fn json_boundary_and_writer_refusal_agree_before_allocation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("boundary.json");
        let accepted = "x".repeat((JSON_BYTES - 3) as usize);
        crate::artifacts::atomic(&path, &accepted).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), JSON_BYTES);
        assert_eq!(read_json::<String>(&path, JSON_BYTES).unwrap(), accepted);
        let rejected = "x".repeat((JSON_BYTES - 2) as usize);
        assert!(is_exceeded(
            &crate::artifacts::atomic(&path, &rejected).unwrap_err()
        ));
        assert_eq!(read_json::<String>(&path, JSON_BYTES).unwrap(), accepted);
        let mut writer = Writer::new(Vec::new(), 4);
        writer.write_all(b"1234").unwrap();
        assert_eq!(
            writer.write_all(b"5").unwrap_err().kind(),
            std::io::ErrorKind::FileTooLarge
        );
        assert_eq!(writer.written, 4);
    }

    #[test]
    fn evidence_array_refuses_extra_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("array.json");
        std::fs::write(&path, b"[1,2,3]").unwrap();
        assert_eq!(read_array::<u64>(&path, JSON_BYTES, 3).unwrap(), [1, 2, 3]);
        assert!(
            read_array::<u64>(&path, JSON_BYTES, 2)
                .unwrap_err()
                .to_string()
                .contains("row limit")
        );
    }
}
