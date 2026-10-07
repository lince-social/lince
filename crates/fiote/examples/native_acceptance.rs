use fiote::{
    communication::{
        auth::{Account, Auth, MemoryCredential, Session},
        chatgpt::ChatGpt,
    },
    config::Settings,
    provider::{Message, Provider},
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let requested = match arguments.as_slice() {
        [] => Some("gpt-6.1-sol".to_string()),
        [flag] if flag == "--default-model" => None,
        [flag, model] if flag == "--model" && !model.trim().is_empty() => Some(model.clone()),
        _ => {
            return Err(
                "Use --model MODEL or --default-model to choose the live test model.".into(),
            );
        }
    };
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let auth = Auth::production()?;
    let attempt = auth
        .begin(fiote::communication::auth::host_id(directory.path())?, None)
        .await?;
    println!(
        "Open this URL in your normal browser to authorize this isolated native acceptance test:\n{}",
        attempt.url
    );
    let credential = attempt.finish().await?;
    let store = Arc::new(MemoryCredential(Mutex::new(credential)));
    let result = async {
        let provider = ChatGpt::new(
            Settings::default(),
            Session {
                auth: auth.clone(),
                store: store.clone(),
                lock: Default::default(),
            },
        )?;
        let models = provider.models().await?;
        println!(
            "Native login verified. Account models: {}",
            serde_json::to_string(&models).map_err(|error| error.to_string())?
        );
        let model = if let Some(requested) = &requested {
            models
                .iter()
                .find(|model| &model.id == requested)
                .ok_or_else(|| format!("This account does not advertise {requested}; no alternative model was used."))?
        } else {
            models.first().ok_or("This account does not advertise a model.")?
        };
        println!("Live inference model: {}", model.id);
        let settings = Settings {
            model: model.id.clone(),
            context_budget_bytes: model.context_budget_bytes,
            reasoning: model
                .reasoning
                .iter()
                .find(|effort| effort.as_str() == "low")
                .cloned(),
            fast: false,
            ..Default::default()
        };
        let provider = ChatGpt::new(
            settings,
            Session {
                auth: auth.clone(),
                store: store.clone(),
                lock: Default::default(),
            },
        )?;
        let reply = provider
            .complete(
                "This is Lince's native Fiote acceptance test. Do not call tools.",
                &[Message::User(
                    "Say exactly: Hello from native Fiote.".into(),
                )],
                &[],
            )
            .await?;
        if !reply.calls.is_empty() || reply.text.trim().is_empty() {
            return Err("The acceptance request did not produce a completed text response.".into());
        }
        println!("Completed native response: {}", reply.text);
        println!(
            "Usage: {}",
            serde_json::to_string(&reply.usage).map_err(|error| error.to_string())?
        );
        Ok(())
    }
    .await;
    let credential = store.0.lock().await.clone();
    let account = Account::decode(&credential)?;
    if auth.revoke(&account).await.is_err() {
        eprintln!(
            "The temporary test session's remote revocation was not confirmed. Disconnect this test registration in ChatGPT settings if needed."
        );
    } else {
        println!("Temporary native test session revoked.");
    }
    result
}
