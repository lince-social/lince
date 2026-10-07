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
