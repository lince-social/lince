use crate::provider::ToolDefinition;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Component, Path},
    sync::Arc,
};

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn run(&self, arguments: Value) -> Result<Value, String>;
}

#[derive(Default)]
pub struct Registry(BTreeMap<String, Arc<dyn Tool>>);

impl Registry {
    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.0.insert(tool.definition().name, Arc::new(tool));
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.0.values().map(|tool| tool.definition()).collect()
    }

    pub async fn run(&self, name: &str, arguments: Value) -> Value {
        let result = match self.0.get(name) {
            Some(tool) => tool.run(arguments).await,
            None => Err(format!("Tool {name} is not available.")),
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
            description: "Create a new UTF-8 file in the configured folder. Use a relative path. Parent folders must already exist. Existing files are never replaced.".into(),
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
            return Err("Use a relative file path inside the configured folder.".into());
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
            Ok(json!({"path": args.path, "bytes": args.content.len()}))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
