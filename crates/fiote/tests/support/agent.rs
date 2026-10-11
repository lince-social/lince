use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn send(value: Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}

fn update(text: &str) {
    send(
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}}),
    );
}

fn main() {
    if std::env::args().any(|arg| arg == "--terminal-login") {
        assert_eq!(std::env::var("TEST_AUTH_ENV").unwrap(), "advertised");
        println!("Private login prompt");
        std::io::stdout().flush().unwrap();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).unwrap();
        if answer.trim() != "confirm" {
            std::process::exit(2);
        }
        std::fs::write("terminal-signed-in", "yes").unwrap();
        return;
    }
    let login_state = std::env::var_os("TEST_LOGIN_STATE").map(std::path::PathBuf::from);
    let log_path = std::env::var("TEST_REQUEST_LOG").ok().or_else(|| {
        std::env::args().find_map(|arg| arg.strip_prefix("--log=").map(str::to_string))
    });
    let mut input = std::io::stdin().lock().lines();
    let mut directory = std::path::PathBuf::new();
    let defaults = json!([
        {"id":"provider","name":"Provider","type":"select","currentValue":"one","options":[{"value":"one","name":"One"},{"value":"two","name":"Two"}]},
        {"id":"model","name":"Model","category":"model","type":"select","currentValue":"large","options":[{"group":"models","name":"Models","options":[{"value":"large","name":"Large"},{"value":"small","name":"Small"}]}]},
        {"id":"thinking","name":"Thinking level","category":"thought_level","type":"select","currentValue":"low","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]},
        {"id":"speed","name":"Fast mode","type":"boolean","currentValue":false}
    ]);
    let mut options = defaults.clone();
    while let Some(Ok(line)) = input.next() {
        let request: Value = serde_json::from_str(&line).unwrap();
        let id = &request["id"];
        let params = &request["params"];
        if let Some(path) = &log_path {
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            writeln!(log, "{}", request["method"].as_str().unwrap()).unwrap();
        }
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => {
                assert!(
                    params["clientCapabilities"]["session"]["configOptions"]["boolean"].is_object()
                );
                json!({"protocolVersion":1,"agentInfo":{"name":"test-agent","version":"1"},"agentCapabilities":{"sessionCapabilities":if std::env::var_os("TEST_DIRECTORIES").is_some() { json!({"additionalDirectories":{}}) } else { json!({}) },"promptCapabilities":{"image":true,"audio":std::env::var_os("TEST_AUDIO").is_some(),"embeddedContext":true},"loadSession":true,"mcpCapabilities":{"http":std::env::var_os("TEST_NO_HTTP").is_none()}},"authMethods":[{"id":"browser","name":"Browser login"}]})
            }
            "test/state" => {
                options[1]["currentValue"] = "small".into();
                for update in [
                    json!({"sessionUpdate":"config_option_update","configOptions":options}),
                    json!({"sessionUpdate":"available_commands_update","availableCommands":[{"name":"review","description":"Review changes"}]}),
                    json!({"sessionUpdate":"plan","entries":[{"content":"Review changes","priority":"high","status":"pending"}]}),
                    json!({"sessionUpdate":"usage_update","used":100,"size":1000,"cost":{"amount":0.01,"currency":"USD"}}),
                ] {
                    send(
                        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":update}}),
                    );
                }
                send(
                    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"other-session","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"other","description":"Another session"}]}}}),
                );
                json!({})
            }
            "test/url" => {
                send(
                    json!({"jsonrpc":"2.0","id":"private-url","method":"elicitation/create","params":{"requestId":id,"mode":"url","message":"Authorize account","url":"https://example.test/login?secret=private-token","elicitationId":"url-one"}}),
                );
                let answer: Value = serde_json::from_str(&input.next().unwrap().unwrap()).unwrap();
                assert_eq!(answer["id"], "private-url");
                answer["result"].clone()
            }
            "test/question" => {
                send(
                    json!({"jsonrpc":"2.0","id":"private-question","method":"elicitation/create","params":{"requestId":id,"mode":"form","message":"Choose a direction","requestedSchema":{"type":"object","properties":{"choice":{"type":"string","enum":["left","right"]}},"required":["choice"]}}}),
                );
                let answer: Value = serde_json::from_str(&input.next().unwrap().unwrap()).unwrap();
                assert_eq!(answer["id"], "private-question");
                answer["result"].clone()
            }
            "authenticate" => {
                assert_eq!(params["methodId"], "browser");
                if let Some(path) = &login_state {
                    std::fs::write(path, "signed-in").unwrap();
                }
                json!({})
            }
            "session/new" | "session/load" => {
                if let Ok(path) = std::env::var("TEST_SESSION_LOG") {
                    let mut log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .unwrap();
                    writeln!(log, "{params}").unwrap();
                }
                if login_state.as_ref().is_some_and(|path| !path.exists()) {
                    send(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Authentication required"}}),
                    );
                    continue;
                }
                directory = params["cwd"].as_str().unwrap().into();
                assert!(directory.is_absolute());
                options = defaults.clone();
                if let Some(server) = params["mcpServers"].as_array().unwrap().first() {
                    let token = if server["type"] == "http" {
                        assert_eq!(server["headers"][0]["name"], "Authorization");
                        server["headers"][0]["value"]
                            .as_str()
                            .unwrap()
                            .strip_prefix("Bearer ")
                            .unwrap()
                    } else {
                        assert!(server.get("type").is_none());
                        assert!(
                            std::path::Path::new(server["command"].as_str().unwrap()).is_absolute()
                        );
                        assert!(std::env::var_os("TEST_NO_HTTP").is_some());
                        assert_eq!(server["args"], json!(["--mcp-stdio"]));
                        let env = server["env"].as_array().unwrap();
                        assert!(env.iter().any(|entry| entry["name"] == "LINCE_MCP_URL"));
                        env.iter()
                            .find(|entry| entry["name"] == "LINCE_MCP_TOKEN")
                            .unwrap()["value"]
                            .as_str()
                            .unwrap()
                    };
                    if std::env::var_os("TEST_HOST_WORKFLOW").is_some() {
                        assert!(token.len() > 16);
                    } else {
                        assert_eq!(token, "test-token");
                    }
                }
                if request["method"] == "session/load" {
                    for _ in 0..1024 {
                        update("old reply");
                    }
                    json!({"configOptions":options})
                } else {
                    json!({"sessionId":"test-session","configOptions":options})
                }
            }
            "session/set_config_option" => {
                let option = options
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|option| option["id"] == params["configId"])
                    .unwrap();
                option["currentValue"] = params["value"].clone();
                if params["configId"] == "speed" {
                    assert_eq!(params["type"], "boolean");
                }
                if params["configId"] == "model" && params["value"] == "small" {
                    options[2]["currentValue"] = "low".into();
                    options[2]["options"] = json!([{"value":"low","name":"Low"}]);
                }
                std::fs::write(directory.join("options.json"), options.to_string()).unwrap();
                json!({"configOptions":options})
            }
            "session/prompt" => {
                if let Ok(path) = std::env::var("TEST_PROMPT_PATH") {
                    std::fs::write(path, params.to_string()).unwrap();
                }
                assert!(
                    std::env::var_os("TEST_CHECK_ONLY").is_none(),
                    "Connection checks must never prompt a model"
                );
                if params["prompt"][0]["text"] == "question"
                    || std::env::var_os("TEST_QUESTION").is_some()
                {
                    send(
                        json!({"jsonrpc":"2.0","id":"shared-question","method":"elicitation/create","params":{"sessionId":"test-session","mode":"form","message":"Choose a direction","requestedSchema":{"type":"object","properties":{"choice":{"type":"string","enum":["left","right"]}},"required":["choice"]}}}),
                    );
                    let answer: Value =
                        serde_json::from_str(&input.next().unwrap().unwrap()).unwrap();
                    if answer["method"] == "session/cancel" {
                        send(json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"cancelled"}}));
                        continue;
                    }
                    assert_eq!(answer["id"], "shared-question");
                    if let Ok(path) = std::env::var("TEST_ANSWER_PATH") {
                        std::fs::write(path, answer.to_string()).unwrap();
                    }
                    update("Answer received.");
                    send(json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"end_turn"}}));
                    continue;
                }
                if params["prompt"][0]["text"] == "timeline" {
                    for (message, text) in [("first", "Inspecting "), ("first", "files.")] {
                        send(
                            json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":{"sessionUpdate":"agent_message_chunk","messageId":message,"content":{"type":"text","text":text}}}}),
                        );
                    }
                    send(
                        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":{"sessionUpdate":"tool_call","toolCallId":"read","title":"Read log","status":"completed","content":[{"type":"content","content":{"type":"text","text":"passed"}}]}}}),
                    );
                    send(
                        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":{"sessionUpdate":"agent_message_chunk","messageId":"final","content":{"type":"text","text":"Done."}}}}),
                    );
                    send(json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"end_turn"}}));
                    continue;
                }
                update("Hello");
                update(" from ");
                send(
                    json!({"jsonrpc":"2.0","method":"session/request_permission","id":"permission","params":{"sessionId":"test-session","toolCall":{"toolCallId":"write","title":"Create hello.txt","kind":"edit","rawInput":{"path":"hello.txt"}},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}}),
                );
                let answer: Value = serde_json::from_str(&input.next().unwrap().unwrap()).unwrap();
                if answer["method"] == "session/cancel" {
                    send(json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"cancelled"}}));
                    continue;
                }
                if answer["result"]["outcome"]["optionId"] == "allow" {
                    std::fs::write(directory.join("hello.txt"), "Hello from the agent").unwrap();
                    update("agent");
                    send(
                        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"test-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"write","title":"Created hello.txt","status":"completed"}}}),
                    );
                    json!({"stopReason":"end_turn"})
                } else {
                    update("cancelled tool");
                    json!({"stopReason":"cancelled"})
                }
            }
            _ => json!({}),
        };
        if !id.is_null() {
            send(json!({"jsonrpc":"2.0","id":id,"result":result}));
        }
    }
}
