use nucleus::{Cause, CauseKind, NewFact};

struct Origin {
    agent: String,
    thread: String,
    message: Option<String>,
}

tokio::task_local! {
    static ORIGIN: Origin;
}

pub async fn fiote<F: std::future::Future>(agent: &str, thread: &str, work: F) -> F::Output {
    fiote_from_message(agent, thread, None, work).await
}

pub async fn fiote_from_message<F: std::future::Future>(
    agent: &str,
    thread: &str,
    message: Option<&str>,
    work: F,
) -> F::Output {
    ORIGIN
        .scope(
            Origin {
                agent: agent.into(),
                thread: thread.into(),
                message: message.map(str::to_string),
            },
            work,
        )
        .await
}

pub(crate) fn stamp(fact: &mut NewFact) {
    if matches!(
        fact.cause.kind,
        CauseKind::UserEdit | CauseKind::TextEdit | CauseKind::Action
    ) {
        let _ = ORIGIN.try_with(|origin| {
            let mut payload = fact
                .payload
                .as_deref()
                .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| match &fact.payload {
                    Some(value) => serde_json::json!({"original_payload":value}),
                    None => serde_json::json!({}),
                });
            payload["fiote"] = serde_json::json!({"agent":origin.agent,"thread":origin.thread});
            if let Some(message) = &origin.message {
                payload["fiote"]["message"] = serde_json::json!(message);
            }
            fact.payload = Some(payload.to_string());
            fact.cause = Cause {
                kind: CauseKind::Fiote,
                uid: Some(origin.thread.clone()),
            };
        });
    }
}

pub(crate) fn is_fiote() -> bool {
    ORIGIN.try_with(|_| ()).is_ok()
}

pub(crate) fn component_origin() -> Option<nucleus::component::composition::Origin> {
    ORIGIN
        .try_with(|origin| nucleus::component::composition::Origin {
            agent: origin.agent.clone(),
            thread: origin.thread.clone(),
        })
        .ok()
}
