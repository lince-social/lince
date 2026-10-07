use async_trait::async_trait;
use engine::{Engine, actions::Action};
use fiote::{
    communication::{
        auth::{Account, Auth, MemoryCredential, Session},
        chatgpt::ChatGpt,
    },
    config::Settings,
    provider::{Message, ToolDefinition},
    tools::{Registry, Tool},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Mutex;

const HEAD: &str = "Native Fiote acceptance";
const BODY: &str = "Created by a real model through Lince Actions.";

struct ScopedTool {
    definition: ToolDefinition,
    native: Arc<Registry>,
    writes: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for ScopedTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    async fn run(&self, arguments: Value) -> Result<Value, String> {
        if self.definition.name == "lince_action" {
            let action: Value = serde_json::from_str(
                arguments["action"]
                    .as_str()
                    .ok_or("Encode the Action as JSON.")?,
            )
            .map_err(|error| error.to_string())?;
            let expected = serde_json::to_value(Action::CreateRecord {
                slug: Some("native-acceptance".into()),
                kind: nucleus::RecordKind::Plain,
                head: HEAD.into(),
                body: BODY.into(),
                quantity: 0.0,
            })
            .map_err(|error| error.to_string())?;
            let parsed: Action =
                serde_json::from_value(action).map_err(|error| error.to_string())?;
            if serde_json::to_value(parsed).map_err(|error| error.to_string())? != expected {
                return Err(
                    "This isolated acceptance test permits only the requested fixture Record."
                        .into(),
                );
            }
        } else if arguments != json!({"action":"create-record"}) {
            return Err("Discover only create-record in this isolated acceptance test.".into());
        }
        let result = self.native.run(&self.definition.name, arguments).await;
        if result["ok"] != true {
            return Err(result["error"]
                .as_str()
                .unwrap_or("The native Action failed.")
                .into());
        }
        if self.definition.name == "lince_action" {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
        Ok(result["result"].clone())
    }
}

async fn verify(provider: &ChatGpt) -> Result<(), String> {
    let engine = Arc::new(
        Engine::open_memory()
            .await
            .map_err(|error| error.to_string())?,
    );
    let agent = engine
        .act(
            Action::CreateAgent {
                head: "Native test Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("Fiote fixture was not created.")?;
    let thread = engine
        .act(
            Action::CreateThread {
                target: agent.clone(),
                head: "Native acceptance".into(),
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("Thread fixture was not created.")?;
    let native = transport::Session::local(
        engine.clone(),
        Arc::new(transport::LaneHub::new()),
        "native-acceptance",
    )
    .into_native_tools(transport::native::Context {
        agent: agent.clone(),
        record: agent,
        thread,
    });
    let mut registry = Registry::default();
    native.register(&mut registry);
    let registry = Arc::new(registry);
    let writes = Arc::new(AtomicUsize::new(0));
    let mut scoped = Registry::default();
    for definition in registry
        .definitions()
        .into_iter()
        .filter(|definition| matches!(definition.name.as_str(), "lince_describe" | "lince_action"))
    {
        scoped.register(ScopedTool {
            definition,
            native: registry.clone(),
            writes: writes.clone(),
        });
    }
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    let result = fiote::runtime::run(
        provider,
        "This is an isolated Lince acceptance test. Discover create-record through lince_describe, then use lince_action exactly once. Report success only after its receipt. Do not retry an uncertain write. No other operation is authorized.",
        vec![Message::User(format!("Create one plain Record with slug native-acceptance, head {HEAD:?}, body {BODY:?}, quantity 0. Use request_id native-acceptance and an empty read_ids list. Finish with a short confirmation."))],
        &scoped,
        receiver,
    ).await;
    native.close().await;
    let reply = result?;
    let rows: Vec<(String, String)> = store::sqlx::query_as(
        "SELECT head, body FROM record WHERE slug = 'native-acceptance' AND deleted_at IS NULL",
    )
    .fetch_all(&engine.store.pool)
    .await
    .map_err(|error| error.to_string())?;
    if writes.load(Ordering::SeqCst) != 1
        || rows != vec![(HEAD.into(), BODY.into())]
        || reply.trim().is_empty()
    {
        return Err(
            "The real model did not complete exactly one verified native fixture Action.".into(),
        );
    }
    println!("Verified one real model-driven Lince Action and its stored Record.");
    println!("Completed tool workflow response: {reply}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let requested = match arguments.as_slice() {
        [flag] if flag == "--default-model" => None,
        [flag, model] if flag == "--model" && !model.trim().is_empty() => Some(model.clone()),
        _ => {
            return Err(
                "Choose --model MODEL or --default-model for this real-account test.".into(),
            );
        }
    };
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let auth = Auth::production()?;
    let attempt = auth
        .begin(fiote::communication::auth::host_id(directory.path())?, None)
        .await?;
    println!(
        "Authorize this isolated native Action acceptance test in your normal browser:\n{}",
        attempt.url
    );
    let store = Arc::new(MemoryCredential(Mutex::new(attempt.finish().await?)));
    let result = async {
        let session = Session {
            auth: auth.clone(),
            store: store.clone(),
            lock: Default::default(),
        };
        let models = ChatGpt::new(Settings::default(), session.clone())?
            .models()
            .await?;
        let model = if let Some(requested) = &requested {
            models
                .iter()
                .find(|model| &model.id == requested)
                .ok_or("The requested model is not advertised by this account.")?
        } else {
            models
                .first()
                .ok_or("The account does not advertise a model.")?
        };
        println!("Live native Action model: {}", model.id);
        let provider = ChatGpt::new(
            Settings {
                model: model.id.clone(),
                reasoning: model
                    .reasoning
                    .iter()
                    .find(|effort| effort.as_str() == "low")
                    .cloned(),
                context_budget_bytes: model.context_budget_bytes,
                ..Default::default()
            },
            session,
        )?;
        verify(&provider).await
    }
    .await;
    let account = Account::decode(&store.0.lock().await.clone())?;
    if auth.revoke(&account).await.is_err() {
        eprintln!(
            "Remote revocation was not confirmed. Disconnect this temporary test registration in ChatGPT settings if needed."
        );
    } else {
        println!("Temporary native test session revoked.");
    }
    result
}
