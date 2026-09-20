use crate::{
    adapters::{AuthKind, AuthMethod, Descriptor},
    config::{ProviderKind, Secret, Settings},
    provider::{Message, Reply, ToolCall, ToolDefinition},
};
use modelbridge::{
    Provider as _,
    oauth::{Store, Subscription, Tokens},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub fn descriptors() -> Vec<Descriptor> {
    Subscription::ALL
        .into_iter()
        .map(|subscription| Descriptor {
            id: ProviderKind(format!("subscription:{}", subscription.id())),
            label: subscription.name().into(),
            endpoint: String::new(),
            auth_methods: vec![AuthMethod {
                id: "browser".into(),
                label: "Browser login".into(),
                kind: AuthKind::Browser,
            }],
            model_optional: true,
        })
        .collect()
}

fn subscription(settings: &Settings) -> Result<Subscription, String> {
    settings
        .provider
        .0
        .strip_prefix("subscription:")
        .and_then(Subscription::parse)
        .ok_or_else(|| "Unknown subscription adapter.".into())
}

struct Login {
    url: Arc<Mutex<Option<String>>>,
    result: Arc<Mutex<Option<Result<Secret, String>>>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Login {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[derive(Deserialize)]
struct Completion {
    settings: Settings,
    credential: Secret,
    system: String,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
}

fn complete(input: Completion) -> Result<Value, String> {
    let subscription = subscription(&input.settings)?;
    let tokens: Tokens = serde_json::from_str(&input.credential.0)
        .map_err(|_| "Invalid subscription credential.")?;
    let store = Store::at(state_directory()?.join("credential.json"));
    store
        .put_tokens(subscription.id(), Some(tokens.clone()))
        .map_err(|_| "Cannot prepare subscription credentials.")?;
    let session = subscription.session(&store, tokens);
    let model = if input.settings.model.is_empty() {
        subscription.default_model()
    } else {
        &input.settings.model
    };
    let client = subscription
        .client(session, model)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| "Cannot connect the subscription provider.")?;
    let system = subscription.system_prefix().map_or_else(
        || input.system.clone(),
        |prefix| format!("{prefix}\n\n{}", input.system),
    );
    let messages = input.messages.iter().map(|message| match message {
        Message::User(text) => modelbridge::Message::user(text),
        Message::Assistant { text, calls } => modelbridge::Message::tool_calls(
            text,
            calls
                .iter()
                .map(|call| modelbridge::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    args: call.arguments.clone(),
                })
                .collect(),
        ),
        Message::Tool { id, name, result } => {
            modelbridge::Message::tool_results(vec![modelbridge::ToolResult {
                id: id.clone(),
                name: name.clone(),
                output: result.to_string(),
                failed: result["ok"] == false,
            }])
        }
    });
    let mut request =
        modelbridge::Request::new(system)
            .messages(messages)
            .tools(input.tools.iter().map(|tool| {
                modelbridge::ToolSchema::new(&tool.name, &tool.description, tool.schema.clone())
            }));
    if subscription.accepts_output_limit() {
        request = request.max_output_tokens(4096);
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || client.stream(&request, &sender));
    let mut reply = Reply::default();
    let mut finished = false;
    let mut failed = false;
    for chunk in receiver {
        match chunk {
            modelbridge::Chunk::Text(text) => reply.text.push_str(&text),
            modelbridge::Chunk::Tool(call) => reply.calls.push(ToolCall {
                id: call.id,
                name: call.name,
                arguments: call.args,
                signatures: None,
            }),
            modelbridge::Chunk::Done { .. } => finished = true,
            modelbridge::Chunk::Failed(_) => failed = true,
        }
        if reply.text.len() > 1_048_576 || reply.calls.len() > crate::runtime::MAX_TOOL_CALLS {
            return Err("Subscription response exceeded its limit.".into());
        }
    }
    let interrupted = worker
        .join()
        .map_err(|_| "Subscription worker stopped.")?
        .is_err()
        || failed
        || !finished;
    let credential = store
        .load()
        .tokens(subscription.id())
        .cloned()
        .ok_or("Subscription credentials are missing.")?;
    if interrupted {
        return Ok(json!({"credential":Secret(serde_json::to_string(&credential).map_err(|_| "Cannot save refreshed credentials.")?),"error":"The subscription request did not complete."}));
    }
    Ok(
        json!({"reply":reply,"credential":Secret(serde_json::to_string(&credential).map_err(|_| "Cannot save refreshed credentials.")?)}),
    )
}

fn dispatch(method: &str, params: Value, login: &mut Option<Login>) -> Result<Value, String> {
    match method {
        "initialize" => Ok(json!({"protocol":"lince.provider.v1"})),
        "providers/list" => serde_json::to_value(descriptors()).map_err(|e| e.to_string()),
        "login/start" => {
            let settings: Settings = serde_json::from_value(params["settings"].clone())
                .map_err(|_| "Invalid login settings.")?;
            let subscription = subscription(&settings)?;
            if login.is_some() {
                return Err("Login is already pending.".into());
            }
            let pending = Login {
                url: Default::default(),
                result: Default::default(),
                cancel: Default::default(),
            };
            let (url, result, cancel) = (
                pending.url.clone(),
                pending.result.clone(),
                pending.cancel.clone(),
            );
            let store = Store::at(state_directory()?.join("credential.json"));
            std::thread::spawn(move || {
                let outcome = subscription
                    .login("Lince")
                    .run(
                        &store,
                        &|address| {
                            *url.lock().unwrap() = Some(address.into());
                        },
                        &cancel,
                    )
                    .map_err(|_| "Browser login failed or was cancelled.".to_string())
                    .and_then(|tokens| {
                        serde_json::to_string(&tokens)
                            .map(Secret)
                            .map_err(|_| "Cannot save subscription credentials.".into())
                    });
                *result.lock().unwrap() = Some(outcome);
            });
            let deadline = std::time::Instant::now();
            while deadline.elapsed() < Duration::from_secs(20) {
                let address = pending.url.lock().unwrap().clone();
                if let Some(url) = address {
                    let value = json!({"url":url});
                    *login = Some(pending);
                    return Ok(value);
                }
                if pending.result.lock().unwrap().is_some() {
                    return Err(
                        "Cannot start browser login. Check whether another login is open.".into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err("Browser login did not start.".into())
        }
        "login/poll" => {
            let pending = login.as_ref().ok_or("No browser login is pending.")?;
            match pending.result.lock().unwrap().take() {
                Some(result) => Ok(json!({"state":"complete","credential":result?})),
                None => Ok(json!({"state":"pending"})),
            }
        }
        "provider/complete" => {
            complete(serde_json::from_value(params).map_err(|_| "Invalid provider request.")?)
        }
        _ => Err("Unknown provider adapter method.".into()),
    }
}

fn state_directory() -> Result<std::path::PathBuf, String> {
    std::env::var_os("LINCE_PROVIDER_STATE")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "Provider adapter requires a private state directory.".into())
}

pub fn serve() -> Result<(), std::io::Error> {
    let input = std::io::stdin();
    let mut input = input.lock();
    let mut output = std::io::stdout().lock();
    let mut login = None;
    loop {
        let mut bytes = Vec::new();
        loop {
            let available = input.fill_buf()?;
            if available.is_empty() {
                return Ok(());
            }
            let end = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|n| n + 1);
            let count = end.unwrap_or(available.len());
            if bytes.len() + count > 4 * 1024 * 1024 {
                return Err(std::io::Error::other("Provider request is too large."));
            }
            bytes.extend_from_slice(&available[..count]);
            input.consume(count);
            if end.is_some() {
                break;
            }
        }
        let request: Value = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        let result = dispatch(
            request["method"].as_str().unwrap_or(""),
            request["params"].clone(),
            &mut login,
        );
        let reply = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
            Err(error) => {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":error}})
            }
        };
        serde_json::to_writer(&mut output, &reply)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_choices_come_from_the_provider_library() {
        let descriptors = descriptors();
        assert_eq!(descriptors.len(), Subscription::ALL.len());
        for (descriptor, subscription) in descriptors.iter().zip(Subscription::ALL) {
            assert_eq!(descriptor.label, subscription.name());
            assert!(descriptor.model_optional);
            assert_eq!(descriptor.auth_methods[0].kind, AuthKind::Browser);
            let settings = Settings {
                provider: descriptor.id.clone(),
                ..Default::default()
            };
            assert_eq!(super::subscription(&settings).unwrap(), subscription);
        }
    }
}
