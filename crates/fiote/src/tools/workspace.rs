use super::{Registry, Tool};
use crate::provider::ToolDefinition;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Component, Path},
    sync::Arc,
};

const MAX_BYTES: usize = 1_048_576;

#[derive(Clone, Copy)]
enum Operation {
    Read,
    Write,
    Edit,
    List,
}

struct FileTool {
    directory: Arc<cap_std::fs::Dir>,
    operation: Operation,
}

pub(super) fn register(registry: &mut Registry, root: &Path) -> Result<(), String> {
    let directory = Arc::new(
        cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())
            .map_err(|e| e.to_string())?,
    );
    for operation in [
        Operation::Read,
        Operation::Write,
        Operation::Edit,
        Operation::List,
    ] {
        registry.register(FileTool {
            directory: directory.clone(),
            operation,
        });
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArguments {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArguments {
    path: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArguments {
    path: String,
    old_text: String,
    new_text: String,
    #[serde(default)]
    replace_all: bool,
}

fn path(value: &str, root_allowed: bool) -> Result<&Path, String> {
    if root_allowed && value == "." {
        return Ok(Path::new(value));
    }
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 4096
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Use a relative path inside the working directory.".into());
    }
    Ok(path)
}

fn read(directory: &cap_std::fs::Dir, path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    directory
        .open(path)
        .map_err(|e| e.to_string())?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("File exceeds the 1 MiB text limit. Read a portion with run_command.".into());
    }
    String::from_utf8(bytes)
        .map_err(|_| "This file is not UTF-8 text. Inspect it with run_command.".into())
}

fn write(directory: &cap_std::fs::Dir, path: &Path, content: &str) -> Result<(), String> {
    if content.len() > MAX_BYTES {
        return Err("File content exceeds the 1 MiB limit.".into());
    }
    directory.write(path, content).map_err(|e| e.to_string())
}

#[async_trait]
impl Tool for FileTool {
    fn definition(&self) -> ToolDefinition {
        let (name, description, properties, required) = match self.operation {
            Operation::Read => (
                "read_file",
                "Read a UTF-8 file relative to the working directory (up to 1 MiB).",
                json!({"path":{"type":"string"}}),
                vec!["path"],
            ),
            Operation::Write => (
                "write_file",
                "Create or overwrite a UTF-8 file relative to the working directory (up to 1 MiB). Parent directories must exist. Read existing files before changing them.",
                json!({"path":{"type":"string"},"content":{"type":"string"}}),
                vec!["path", "content"],
            ),
            Operation::Edit => (
                "edit_file",
                "Replace exact text in a UTF-8 file relative to the working directory. By default old_text must occur exactly once; replace_all replaces every match. Read the file first.",
                json!({"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"},"replace_all":{"type":"boolean"}}),
                vec!["path", "old_text", "new_text"],
            ),
            Operation::List => (
                "list_directory",
                "List up to 1024 entries relative to the working directory. Use path '.' for the working directory itself.",
                json!({"path":{"type":"string"}}),
                vec!["path"],
            ),
        };
        ToolDefinition {
            name: name.into(),
            description: description.into(),
            schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
        }
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        let directory = self.directory.clone();
        let operation = self.operation;
        tokio::task::spawn_blocking(move || {
            match operation {
                Operation::Read => {
                    let args: PathArguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let content = read(&directory, path(&args.path, false)?)?;
                    Ok(json!({"path":args.path,"content":content}))
                }
                Operation::Write => {
                    let args: WriteArguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    write(&directory, path(&args.path, false)?, &args.content)?;
                    Ok(json!({"path":args.path,"bytes":args.content.len()}))
                }
                Operation::Edit => {
                    let args: EditArguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    if args.old_text.is_empty() {
                        return Err("old_text must not be empty.".into());
                    }
                    let file = path(&args.path, false)?;
                    let content = read(&directory, file)?;
                    let matches = content.matches(&args.old_text).count();
                    if matches == 0 || (!args.replace_all && matches != 1) {
                        return Err(format!("Found {matches} matches. Read the current file and supply unique old_text, or use replace_all."));
                    }
                    let growth = args.new_text.len().saturating_sub(args.old_text.len()).saturating_mul(matches);
                    if content.len().saturating_add(growth) > MAX_BYTES {
                        return Err("Edited file would exceed the 1 MiB limit.".into());
                    }
                    let edited = content.replace(&args.old_text, &args.new_text);
                    write(&directory, file, &edited)?;
                    Ok(json!({"path":args.path,"replacements":matches,"bytes":edited.len()}))
                }
                Operation::List => {
                    let args: PathArguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
                    let mut entries = Vec::new();
                    let mut truncated = false;
                    for entry in directory.read_dir(path(&args.path, true)?).map_err(|e| e.to_string())? {
                        if entries.len() == 1024 {
                            truncated = true;
                            break;
                        }
                        let entry = entry.map_err(|e| e.to_string())?;
                        let kind = entry.file_type().map_err(|e| e.to_string())?;
                        entries.push(json!({"name":entry.file_name().to_string_lossy(),"kind":if kind.is_dir() { "directory" } else if kind.is_file() { "file" } else { "other" }}));
                    }
                    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                    Ok(json!({"path":args.path,"entries":entries,"truncated":truncated}))
                }
            }
        }).await.map_err(|e| e.to_string())?
    }
}
