use fiote::{
    conversation::{self, Summary},
    provider::{Message, Provider, Reply, TextOutput, ToolDefinition},
};
use serde_json::json;
use std::sync::Mutex;

struct Model {
    requests: Mutex<Vec<Vec<Message>>>,
    fail: bool,
}
#[async_trait::async_trait]
impl Provider for Model {
    fn context_budget_bytes(&self) -> usize {
        16384
    }
    async fn complete(
        &self,
        _: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        assert!(tools.is_empty());
        self.requests.lock().unwrap().push(messages.to_vec());
        if self.fail {
            return Err("fixture failed".into());
        }
        Ok(Reply {
            text: "Earlier user goal: preserve completed tool receipts and pending work.".into(),
            ..Default::default()
        })
    }
}
struct Output {
    summaries: Mutex<Vec<(usize, String)>>,
    usages: Mutex<Vec<String>>,
}
#[async_trait::async_trait]
impl TextOutput for Output {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    async fn summary(&self, covered: usize, text: &str) -> Result<(), String> {
        self.summaries.lock().unwrap().push((covered, text.into()));
        Ok(())
    }
    async fn usage(&self, usage: nucleus::operation::Usage) -> Result<(), String> {
        self.usages.lock().unwrap().push(usage.scope);
        Ok(())
    }
}
fn history() -> Vec<Message> {
    let mut messages = Vec::new();
    for index in 0..12 {
        messages.push(Message::User(format!(
            "User goal {index}: {}",
            "past details ".repeat(if index < 6 { 160 } else { 5 })
        )));
        messages.push(Message::Assistant {
            text: format!("Call {index}"),
            calls: vec![fiote::provider::ToolCall {
                id: format!("call-{index}"),
                name: "lince_read_record".into(),
                arguments: json!({"record_uid":"r_fixture"}),
                signatures: None,
            }],
        });
        messages.push(Message::Tool {
            id: format!("call-{index}"),
            name: "lince_read_record".into(),
            result: json!({"ok":true,"receipt":index}),
        });
        messages.push(Message::Assistant {
            text: format!("Finished {index}"),
            calls: Vec::new(),
        });
    }
    messages
}
#[tokio::test]
async fn summaries_preserve_six_whole_recent_turns_and_original_transcript() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let output = Output {
        summaries: Default::default(),
        usages: Default::default(),
    };
    let original = history();
    let mut input = original.clone();
    conversation::prepare(
        &model,
        "Current system prompt, task and activation",
        &mut input,
        &output,
    )
    .await
    .unwrap();
    let (covered, text) = output.summaries.lock().unwrap()[0].clone();
    assert_eq!(covered, 24);
    assert!(matches!(&input[0], Message::Summary { covered: 24, .. }));
    assert_eq!(
        serde_json::to_value(&input[1..]).unwrap(),
        serde_json::to_value(&original[24..]).unwrap()
    );
    assert_eq!(original.len(), 48);
    let saved = Summary {
        covered,
        fingerprint: conversation::fingerprint(&original[..covered]).unwrap(),
        text,
    };
    assert!(saved.apply(&original).unwrap().is_some());
    let mut revised = original.clone();
    revised[0] = Message::User("Changed earlier goal".into());
    assert!(saved.apply(&revised).unwrap().is_none());
    revised.remove(0);
    assert!(saved.apply(&revised).unwrap().is_none());
    let requests = model.requests.lock().unwrap();
    assert!(requests.len() >= 2);
    for request in requests.iter() {
        let Message::User(chunk) = &request[0] else {
            panic!("Expected a summary input");
        };
        for index in 0..6 {
            let occurrences = chunk.matches(&format!("call-{index}")).count();
            assert!(occurrences == 0 || occurrences == 2);
        }
    }
}
#[tokio::test]
async fn summary_failure_preserves_history_and_context() {
    let model = Model {
        requests: Default::default(),
        fail: true,
    };
    let original = history();
    let mut input = original.clone();
    let error = conversation::compact(&model, "Current task", &mut input)
        .await
        .unwrap_err();
    assert!(error.contains("original history"));
    assert_eq!(
        serde_json::to_value(input).unwrap(),
        serde_json::to_value(original).unwrap()
    );
}

#[tokio::test]
async fn follow_up_can_compact_a_large_completed_turn_without_splitting_tool_receipts() {
    let output = Output {
        summaries: Default::default(),
        usages: Default::default(),
    };
    let original = vec![
        Message::User("Complete prior work".into()),
        Message::Assistant {
            text: String::new(),
            calls: vec![fiote::provider::ToolCall {
                id: "completed-call".into(),
                name: "read_file".into(),
                arguments: json!({}),
                signatures: None,
            }],
        },
        Message::Tool {
            id: "completed-call".into(),
            name: "read_file".into(),
            result: json!({"receipt":"completed-call-result","data":"past".repeat(1200)}),
        },
        Message::Assistant {
            text: "Prior work finished; receipts are saved".into(),
            calls: vec![],
        },
        Message::User(format!("Follow up: {}", "now".repeat(4000))),
    ];
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let mut input = original.clone();
    conversation::prepare(&model, "Pinned owner instructions", &mut input, &output)
        .await
        .unwrap();
    assert_eq!(input.len(), 2);
    assert!(matches!(&input[0], Message::Summary { covered: 4, .. }));
    assert_eq!(
        serde_json::to_value(&input[1]).unwrap(),
        serde_json::to_value(&original[4]).unwrap()
    );
    assert_eq!(output.summaries.lock().unwrap()[0].0, 4);
    let requests = model.requests.lock().unwrap();
    let Message::User(text) = &requests[0][0] else {
        panic!("Missing summary input")
    };
    assert!(text.contains("completed-call-result"));
    assert!(text.contains("Prior work finished"));
    assert!(!text.contains("Follow up:"));
    drop(requests);
    let failed = Model {
        requests: Default::default(),
        fail: true,
    };
    let mut preserved = original.clone();
    assert!(
        conversation::compact(&failed, "Pinned owner instructions", &mut preserved)
            .await
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(preserved).unwrap(),
        serde_json::to_value(original).unwrap()
    );
}

#[derive(Default)]
struct Compactions(Mutex<Vec<(Vec<Message>, Vec<Message>)>>);
#[async_trait::async_trait]
impl TextOutput for Compactions {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    async fn compacted(&self, before: &[Message], after: &[Message]) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push((before.to_vec(), after.to_vec()));
        Ok(())
    }
}

#[tokio::test]
async fn one_large_tool_turn_compacts_and_keeps_the_current_user_goal() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let output = Compactions::default();
    let mut messages = vec![Message::User("Finish Park work; never reboot".into())];
    for index in 0..10 {
        messages.push(Message::Assistant {
            text: String::new(),
            calls: vec![fiote::provider::ToolCall {
                id: format!("tool-{index}"),
                name: "read_file".into(),
                arguments: json!({}),
                signatures: None,
            }],
        });
        messages.push(Message::Tool {
            id: format!("tool-{index}"),
            name: "read_file".into(),
            result: json!({"ok":true,"receipt":index,"text":"é".repeat(2000)}),
        });
    }
    let original = messages.clone();
    conversation::prepare(&model, "Pinned permissions", &mut messages, &output)
        .await
        .unwrap();
    assert!(fiote::runtime::context_bytes(&messages).unwrap() < 16384);
    assert!(
        matches!(messages.last(), Some(Message::User(text)) if text == "Finish Park work; never reboot")
    );
    assert_eq!(
        serde_json::to_value(&output.0.lock().unwrap()[0].0).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert!(model.requests.lock().unwrap().len() > 1);
    let saved = conversation::Checkpoint {
        covered: original.len(),
        fingerprint: conversation::fingerprint(&original).unwrap(),
        replacement: messages.clone(),
    };
    let mut follow_up = original.clone();
    follow_up.push(Message::Assistant {
        text: "Finished".into(),
        calls: vec![],
    });
    follow_up.push(Message::User("Continue".into()));
    let restored = saved.apply(&follow_up).unwrap().unwrap();
    assert!(matches!(restored.last(), Some(Message::User(text)) if text == "Continue"));
    follow_up[0] = Message::User("Edited goal".into());
    assert!(saved.apply(&follow_up).unwrap().is_none());
}

struct FailCheckpoint;
#[async_trait::async_trait]
impl TextOutput for FailCheckpoint {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    async fn compacted(&self, _: &[Message], _: &[Message]) -> Result<(), String> {
        Err("fixture disk full".into())
    }
}

#[tokio::test]
async fn failed_checkpoint_or_unfinished_batch_never_replaces_context() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let mut messages = history();
    let original = serde_json::to_value(&messages).unwrap();
    assert!(
        conversation::prepare(&model, "Pinned", &mut messages, &FailCheckpoint)
            .await
            .unwrap_err()
            .contains("disk full")
    );
    assert_eq!(serde_json::to_value(&messages).unwrap(), original);
    messages.push(Message::Assistant {
        text: String::new(),
        calls: vec![fiote::provider::ToolCall {
            id: "pending".into(),
            name: "write".into(),
            arguments: json!({}),
            signatures: None,
        }],
    });
    let original = serde_json::to_value(&messages).unwrap();
    assert!(
        conversation::compact(&model, "Pinned", &mut messages)
            .await
            .unwrap_err()
            .contains("unfinished tool batch")
    );
    assert_eq!(serde_json::to_value(&messages).unwrap(), original);
}

#[tokio::test]
async fn huge_unicode_tool_output_has_a_bounded_projection_and_full_original_receipt() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let original = vec![
        Message::User("Review result".into()),
        Message::Tool {
            id: "receipt".into(),
            name: "read_file".into(),
            result: json!({"ok":true,"text":"🦀é".repeat(100000)}),
        },
    ];
    let output = Compactions::default();
    let mut messages = original.clone();
    conversation::prepare(&model, "Pinned", &mut messages, &output)
        .await
        .unwrap();
    assert!(fiote::runtime::context_bytes(&messages).unwrap() < 16384);
    let captures = output.0.lock().unwrap();
    assert_eq!(
        serde_json::to_value(&captures[0].0).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
}

#[tokio::test]
async fn compaction_preserves_the_current_binary_attachment_byte_for_byte() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let mut input = history();
    let current = Message::RichUser {
        text: "Keep this current image available".into(),
        content: vec![nucleus::message::MessagePart::Attachment {
            name: "synthetic.png".into(),
            mime_type: "image/png".into(),
            data: "AAAA".repeat(200000),
        }],
    };
    input.push(current.clone());
    conversation::compact(&model, "Current permissions", &mut input)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(input.last().unwrap()).unwrap(),
        serde_json::to_value(current).unwrap()
    );
    fiote::runtime::validate_context("Current permissions", &input).unwrap();
}

#[tokio::test]
async fn active_turn_keeps_recent_complete_tool_groups_alongside_the_summary() {
    let model = Model {
        requests: Default::default(),
        fail: false,
    };
    let mut messages = vec![Message::User("Original goal; never reboot".into())];
    for index in 0..15 {
        messages.push(Message::Replay {
            provider: "chatgpt".into(),
            items: vec![json!({"type":"reasoning","encrypted_content":"fixture"})],
        });
        messages.push(Message::Assistant {
            text: String::new(),
            calls: vec![fiote::provider::ToolCall {
                id: index.to_string(),
                name: "read_file".into(),
                arguments: json!({}),
                signatures: None,
            }],
        });
        messages.push(Message::Tool {
            id: index.to_string(),
            name: "read_file".into(),
            result: json!({"data":"x".repeat(1024)}),
        });
    }
    conversation::compact(&model, "Pinned permissions", &mut messages)
        .await
        .unwrap();
    assert!(matches!(messages.first(), Some(Message::Summary { .. })));
    assert_eq!(
        messages
            .iter()
            .filter(|m| matches!(m, Message::User(text) if text == "Original goal; never reboot"))
            .count(),
        1
    );
    assert!(matches!(messages.last(), Some(Message::Tool { id, .. }) if id == "14"));
    let calls: std::collections::HashSet<_> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Assistant { calls, .. } => calls.first().map(|call| call.id.clone()),
            _ => None,
        })
        .collect();
    let results: std::collections::HashSet<_> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Tool { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(calls, results);
    for (index, message) in messages.iter().enumerate() {
        if matches!(message, Message::Assistant { .. }) {
            assert!(matches!(
                messages.get(index - 1),
                Some(Message::Replay { .. })
            ));
        }
    }
}
