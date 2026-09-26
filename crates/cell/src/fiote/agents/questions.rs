use super::*;

#[derive(Serialize, Deserialize)]
struct Anchor {
    message: String,
}

pub(in crate::fiote) async fn recover(engine: &Arc<Engine>, path: &Path) -> Result<(), String> {
    let anchor: Anchor =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if let Some(mut value) =
        store::records::get_extension(&engine.store.pool, &anchor.message, "lince.message-content")
            .await
            .map_err(|error| error.to_string())?
    {
        if value["parts"][0]["question"]["state"] == "pending" {
            value["parts"][0]["question"]["state"] = "cancelled".into();
            engine
                .act(
                    Action::SetExtension {
                        target: anchor.message,
                        namespace: "lince.message-content".into(),
                        fds: value,
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

struct Waiting {
    engine: Arc<Engine>,
    path: PathBuf,
}
impl Drop for Waiting {
    fn drop(&mut self) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let engine = self.engine.clone();
            let path = self.path.clone();
            runtime.spawn(async move {
                let _ = recover(&engine, &path).await;
            });
        }
    }
}

pub(super) async fn ask(
    output: &Output<'_>,
    request: acp::QuestionRequest,
) -> Result<acp::Answer, String> {
    let schema = request
        .schema
        .ok_or("Only form questions are shared in messages.")?;
    let engine = &output.timeline.engine;
    let responder = store::organs::local(&engine.store.pool)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Local account identity is unavailable.")?
        .uid;
    let parent = output.timeline.state.lock().await.message.clone();
    let question = nucleus::question::Question {
        prompt: request.prompt,
        responder,
        schema,
        state: nucleus::question::State::Pending,
        answers: None,
        expires_ms: Some(request.expires_ms),
    };
    let message = engine
        .act(
            Action::CreateMessage {
                thread: output.thread.clone(),
                body: String::new(),
                content: vec![nucleus::message::MessagePart::Question { question }],
                author: Some(output.timeline.author.clone()),
                state: MessageState::Finished,
                parent: Some(parent),
                references: Vec::new(),
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?
        .created
        .ok_or("Could not save the question.")?;
    let directory = output
        .timeline
        .pending
        .parent()
        .ok_or("Question state directory is missing.")?;
    let path = directory.join(format!("question-{}.json", request.id));
    if let Err(error) = save(
        &path,
        &Anchor {
            message: message.clone(),
        },
    ) {
        if let Some(mut value) =
            store::records::get_extension(&engine.store.pool, &message, "lince.message-content")
                .await
                .map_err(|error| error.to_string())?
        {
            value["parts"][0]["question"]["state"] = "cancelled".into();
            let _ = engine
                .act(
                    Action::SetExtension {
                        target: message,
                        namespace: "lince.message-content".into(),
                        fds: value,
                    },
                    None,
                )
                .await;
        }
        return Err(error);
    }
    let _waiting = Waiting {
        engine: engine.clone(),
        path,
    };
    output.timeline.progress(None, "waiting for input").await?;
    loop {
        if nucleus::operation::now_ms() >= request.expires_ms {
            return Ok(acp::Answer::Cancel);
        }
        let value =
            store::records::get_extension(&engine.store.pool, &message, "lince.message-content")
                .await
                .map_err(|error| error.to_string())?
                .ok_or("The question was removed.")?;
        let question: nucleus::question::Question =
            serde_json::from_value(value["parts"][0]["question"].clone())
                .map_err(|error| error.to_string())?;
        let answer = match question.state {
            nucleus::question::State::Answered => Some(acp::Answer::Accept {
                content: question.answers,
            }),
            nucleus::question::State::Declined => Some(acp::Answer::Decline),
            nucleus::question::State::Cancelled => Some(acp::Answer::Cancel),
            nucleus::question::State::Pending => None,
        };
        if let Some(answer) = answer {
            output.timeline.progress(None, "running").await?;
            return Ok(answer);
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

impl Agents {
    pub async fn questions(&self, record: &str) -> Vec<fiote::config::PrivateQuestion> {
        let connections: Vec<_> = self
            .discovery
            .lock()
            .await
            .iter()
            .filter(|(key, _)| key.as_str() == record || key.as_str() == format!("login:{record}"))
            .map(|(_, (_, connection))| (None, connection.clone()))
            .collect();
        let mut connections = connections;
        connections.extend(
            self.sessions
                .lock()
                .await
                .iter()
                .filter(|(_, runtime)| runtime.record == record)
                .map(|(thread, runtime)| (Some(thread.clone()), runtime.connection.clone())),
        );
        let mut result = Vec::new();
        for (thread, connection) in connections {
            for request in connection.questions().await {
                result.push(fiote::config::PrivateQuestion {
                    thread: thread.clone(),
                    request,
                });
            }
        }
        result
    }

    pub async fn answer_question(
        &self,
        record: &str,
        id: &str,
        answer: acp::Answer,
    ) -> Result<(), String> {
        let mut connections: Vec<_> = self
            .discovery
            .lock()
            .await
            .iter()
            .filter(|(key, _)| key.as_str() == record || key.as_str() == format!("login:{record}"))
            .map(|(_, (_, connection))| connection.clone())
            .collect();
        connections.extend(
            self.sessions
                .lock()
                .await
                .values()
                .filter(|runtime| runtime.record == record)
                .map(|runtime| runtime.connection.clone()),
        );
        for connection in connections {
            if connection
                .questions()
                .await
                .iter()
                .any(|question| question.id == id)
            {
                return connection.answer_question(id, answer).await;
            }
        }
        Err("This agent interaction is no longer waiting.".into())
    }
}
