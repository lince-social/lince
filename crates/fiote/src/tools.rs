// non-lean-yet
// Scope: mod command, mod workspace, trait Tool, struct Registry.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites 95.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 27, 57.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 24, 54.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 18, 19,
//   20, 24, 27, 34, 38, 42, 54, 57.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites new: String
//   (57), register_workspace: String (27).

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Component, Path},
    sync::Arc,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::provider::ToolDefinition;

mod changes;
mod command;
pub(crate) mod receipts;
mod workspace;

pub use changes::file_diff;
pub use command::RunCommand;

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn run(&self, arguments: Value) -> Result<Value, String>;
}

#[derive(Default)]
pub struct Registry(BTreeMap<String, Entry>);

enum Entry {
    Tool(Arc<dyn Tool>),
    Command(RunCommand),
}

impl Entry {
    fn tool(&self) -> &dyn Tool {
        match self {
            Self::Tool(tool) => tool.as_ref(),
            Self::Command(command) => command,
        }
    }
}

impl Registry {
    pub fn register_workspace(&mut self, directory: &Path) -> Result<(), String> {
        self.register(CreateFile::new(directory)?);
        workspace::register(self, directory)?;
        let command = RunCommand::new(directory)?;
        self.0
            .insert(command.definition().name, Entry::Command(command));
        Ok(())
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.0
            .insert(tool.definition().name, Entry::Tool(Arc::new(tool)));
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.0
            .values()
            .map(|entry| entry.tool().definition())
            .collect()
    }

    pub async fn run(&self, name: &str, arguments: Value) -> Value {
        self.run_with_receipts(name, arguments, None).await
    }

    pub(crate) async fn run_with_receipts(
        &self,
        name: &str,
        arguments: Value,
        receipts: Option<&receipts::Receipts>,
    ) -> Value {
        let result = match (self.0.get(name), receipts) {
            (Some(Entry::Command(command)), Some(receipts)) => {
                command
                    .run_captured(arguments, Some(&receipts.command()))
                    .await
            }
            (Some(entry), _) => entry.tool().run(arguments).await,
            (None, _) => Err(format!("Tool {name} is not available.")),
        };
        match result {
            Ok(result) => json!({"ok": true, "result": result}),
            Err(error) => json!({"ok": false, "error": error}),
        }
    }
}

pub struct CreateFile(Arc<cap_std::fs::Dir>);

impl CreateFile {
    pub fn new(root: &Path) -> Result<Self, String> {
        cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())
            .map(|dir| Self(Arc::new(dir)))
            .map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileArguments {
    path: String,
    content: String,
}

#[async_trait]
impl Tool for CreateFile {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_file".into(),
            description: "Create a new UTF-8 file relative to the working directory. Parent directories must already exist. Existing files are never replaced; use write_file or edit_file to change them.".into(),
            schema: json!({"type":"object", "properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}),
        }
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        let args: FileArguments = serde_json::from_value(arguments).map_err(|e| e.to_string())?;
        let path = Path::new(&args.path);
        if args.path.is_empty()
            || args.path.len() > 4096
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("Use a relative file path inside the working directory.".into());
        }
        if args.content.len() > 1_048_576 {
            return Err("File content exceeds the 1 MiB limit.".into());
        }
        let dir = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut file = dir
                .open_with(&args.path, &options)
                .map_err(|e| e.to_string())?;
            if let Err(error) = file
                .write_all(args.content.as_bytes())
                .and_then(|()| file.sync_all())
            {
                return Err(format!("File may be incomplete: {error}"));
            }
            Ok(json!({
                "path": args.path,
                "bytes": args.content.len(),
                "diff": file_diff(&args.path, None, Some(&args.content)),
            }))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
