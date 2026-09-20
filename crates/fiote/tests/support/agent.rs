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
    let mut input = std::io::stdin().lock().lines();
    let mut directory = std::path::PathBuf::new();
    while let Some(Ok(line)) = input.next() {
        let request: Value = serde_json::from_str(&line).unwrap();
        let id = &request["id"];
        let params = &request["params"];
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => {
                json!({"protocolVersion":1,"agentInfo":{"name":"test-agent","version":"1"},"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true}},"authMethods":[{"id":"browser","name":"Browser login"}]})
            }
            "authenticate" => {
                assert_eq!(params["methodId"], "browser");
                json!({})
            }
            "session/new" | "session/load" => {
                directory = params["cwd"].as_str().unwrap().into();
                assert!(directory.is_absolute());
                assert_eq!(params["mcpServers"][0]["type"], "http");
                assert_eq!(
                    params["mcpServers"][0]["headers"][0]["name"],
                    "Authorization"
                );
                assert_eq!(
                    params["mcpServers"][0]["headers"][0]["value"],
                    "Bearer test-token"
                );
                if request["method"] == "session/load" {
                    for _ in 0..1024 {
                        update("old reply");
                    }
                    json!({})
                } else {
                    json!({"sessionId":"test-session"})
                }
            }
            "session/prompt" => {
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
            "_goose/unstable/providers/config/authenticate" => {
                send(
                    json!({"jsonrpc":"2.0","method":"_goose/unstable/providers/authentication/device-code","params":{"providerId":"test","userCode":"test-code","verificationUri":"https://example.com/login","expiresIn":600}}),
                );
                json!({})
            }
            _ => json!({}),
        };
        if !id.is_null() {
            send(json!({"jsonrpc":"2.0","id":id,"result":result}));
        }
    }
}
