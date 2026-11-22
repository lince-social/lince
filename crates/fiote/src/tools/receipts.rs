// non-lean-yet
// Scope: struct Receipts, struct CommandCapture, fn read_page, fn preview.

use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tempfile::{NamedTempFile, TempDir};

use crate::{conversation::fingerprint_bytes, provider::ToolDefinition};

pub(crate) const READ_TOOL_OUTPUT: &str = "lince_read_tool_output";
pub(crate) const PAGE_BYTES: usize = 4096;
pub(crate) const MAX_STREAM_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct Receipts {
    directory: PathBuf,
    _temporary: Option<TempDir>,
}

impl Receipts {
    pub(crate) fn new(directory: Option<PathBuf>) -> Result<Self, String> {
        if let Some(directory) = directory {
            return Ok(Self {
                directory,
                _temporary: None,
            });
        }
        let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
        Ok(Self {
            directory: temporary.path().to_path_buf(),
            _temporary: Some(temporary),
        })
    }

    fn path(&self, id: &str, part: Part) -> PathBuf {
        self.directory.join(format!("{id}.{}", part.extension()))
    }

    fn temporary_file(&self) -> io::Result<NamedTempFile> {
        fs::create_dir_all(&self.directory)?;
        NamedTempFile::new_in(&self.directory)
    }

    pub(crate) fn save(&self, result: &str) -> Result<String, String> {
        let id = fingerprint_bytes(result.as_bytes());
        let path = self.path(&id, Part::Result);
        if !path.is_file() {
            let mut file = self.temporary_file().map_err(|error| error.to_string())?;
            file.write_all(result.as_bytes())
                .map_err(|error| error.to_string())?;
            commit(file, &path).map_err(|error| error.to_string())?;
        }
        Ok(id)
    }

    pub(crate) fn command(&self) -> CommandCapture<'_> {
        CommandCapture {
            receipts: self,
            id: uuid::Uuid::new_v4().simple().to_string(),
        }
    }

    pub(crate) fn read(&self, arguments: Value) -> Result<Value, String> {
        let args: ReadArguments =
            serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        if !matches!(args.receipt_id.len(), 32 | 64)
            || !args.receipt_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Use the receipt ID returned by the tool in this conversation.".into());
        }
        let file = File::open(self.path(&args.receipt_id, args.part)).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "This output receipt or stream is unavailable in the current conversation.".into()
            } else {
                error.to_string()
            }
        })?;
        read_page(file, args.offset, args.limit)
    }
}

fn commit(file: NamedTempFile, path: &Path) -> io::Result<()> {
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    if let Some(directory) = path.parent() {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

pub(crate) struct CommandCapture<'a> {
    receipts: &'a Receipts,
    id: String,
}

impl CommandCapture<'_> {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn start(&self) -> io::Result<NamedTempFile> {
        self.receipts.temporary_file()
    }

    pub(crate) fn finish(&self, part: Part, file: NamedTempFile) -> io::Result<()> {
        commit(file, &self.receipts.path(&self.id, part))
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Part {
    #[default]
    Result,
    Stdout,
    Stderr,
}

impl Part {
    fn extension(self) -> &'static str {
        match self {
            Self::Result => "json",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
    receipt_id: String,
    #[serde(default)]
    part: Part,
    #[serde(default)]
    offset: u64,
    limit: Option<usize>,
}

pub(crate) fn definition() -> ToolDefinition {
    ToolDefinition {
        name: READ_TOOL_OUTPUT.into(),
        description: "Read a page of captured tool output from this conversation. Use the returned receipt ID, its part (result, stdout, or stderr), and next_offset to continue.".into(),
        schema: json!({"type":"object","properties":{"receipt_id":{"type":"string"},"part":{"type":"string","enum":["result","stdout","stderr"]},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":PAGE_BYTES}},"required":["receipt_id"],"additionalProperties":false}),
    }
}

pub(crate) fn read_page(
    mut file: File,
    offset: u64,
    limit: Option<usize>,
) -> Result<Value, String> {
    let limit = limit.unwrap_or(PAGE_BYTES);
    if !(1..=PAGE_BYTES).contains(&limit) {
        return Err(format!("Use a page size from 1 to {PAGE_BYTES} bytes."));
    }
    let total = file.metadata().map_err(|error| error.to_string())?.len();
    if offset > total {
        return Err(format!(
            "The output contains {total} bytes. Use a smaller offset."
        ));
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    // A page may need three extra bytes to finish its last UTF-8 character.
    let mut bytes = Vec::with_capacity(limit + 3);
    file.take((limit + 3) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let mut end = limit.min(bytes.len());
    while end < bytes.len() && bytes[end] & 0xc0 == 0x80 {
        end += 1;
    }
    let next = offset + end as u64;
    Ok(
        json!({"offset":offset,"next_offset":(next < total).then_some(next),"total_bytes":total,"content":String::from_utf8_lossy(&bytes[..end])}),
    )
}
