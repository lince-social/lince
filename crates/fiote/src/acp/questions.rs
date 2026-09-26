use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    pub id: String,
    pub prompt: String,
    pub schema: Option<Value>,
    pub url: Option<crate::config::Secret>,
    pub session: Option<String>,
    pub expires_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Answer {
    Accept {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Value>,
    },
    Decline,
    Cancel,
}

pub(super) struct PendingQuestion {
    request: QuestionRequest,
    response: oneshot::Sender<CreateElicitationResponse>,
}

pub(super) type Questions = Arc<Mutex<BTreeMap<String, PendingQuestion>>>;

impl QuestionRequest {
    pub(super) fn from_request(request: &CreateElicitationRequest) -> Result<Self, String> {
        if request.message.len() > 8192 {
            return Err("The question exceeds its text limit.".into());
        }
        let (schema, url) = match &request.mode {
            ElicitationMode::Form(form) => {
                let schema = serde_json::to_value(&form.requested_schema)
                    .map_err(|error| error.to_string())?;
                nucleus::question::validate_schema(&schema)?;
                (Some(schema), None)
            }
            ElicitationMode::Url(mode) => {
                let url = url::Url::parse(&mode.url)
                    .map_err(|_| "The agent supplied an invalid interaction URL.")?;
                let local = url
                    .host_str()
                    .is_some_and(|host| ["localhost", "127.0.0.1", "[::1]"].contains(&host));
                if mode.url.len() > 8192
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.scheme() != "https" && !(local && url.scheme() == "http")
                {
                    return Err("Agent interaction URLs must use HTTPS or localhost HTTP.".into());
                }
                (None, Some(crate::config::Secret(url.into())))
            }
            _ => return Err("This question mode is not supported.".into()),
        };
        let session = match request.scope() {
            ElicitationScope::Session(scope) => Some(scope.session_id.to_string()),
            _ => None,
        };
        Ok(Self {
            id: nucleus::new_uid("question"),
            prompt: request.message.clone(),
            schema,
            url,
            session,
            expires_ms: nucleus::operation::now_ms() + 30 * 60 * 1000,
        })
    }

    pub fn response(&self, answer: Answer) -> Result<CreateElicitationResponse, String> {
        if !matches!(answer, Answer::Cancel) && nucleus::operation::now_ms() >= self.expires_ms {
            return Err("This interaction expired. Request a new one.".into());
        }
        if let Answer::Accept { content } = &answer {
            if let Some(schema) = &self.schema {
                nucleus::question::validate_answers(
                    schema,
                    content.as_ref().ok_or("Answer the form fields first.")?,
                )?;
            } else if content.is_some() {
                return Err("A URL interaction cannot return form contents.".into());
            }
        }
        serde_json::from_value(serde_json::to_value(answer).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
    }
}

pub(super) async fn receive(
    request: CreateElicitationRequest,
    events: &mpsc::Sender<Event>,
    active: &AtomicBool,
    pending: &Questions,
    mut closed: watch::Receiver<bool>,
) -> Result<CreateElicitationResponse, agent_client_protocol::Error> {
    let question = QuestionRequest::from_request(&request)
        .map_err(|error| agent_client_protocol::Error::invalid_params().data(Value::from(error)))?;
    let (response, received) = oneshot::channel();
    let id = question.id.clone();
    let shared =
        question.schema.is_some() && question.session.is_some() && active.load(Ordering::Acquire);
    if shared {
        events
            .send(Event::Question(question, response))
            .await
            .map_err(|_| agent_client_protocol::Error::internal_error())?;
    } else {
        let mut questions = pending.lock().await;
        if questions.len() >= 32 {
            return Err(agent_client_protocol::Error::invalid_params()
                .data(Value::from("Too many pending questions.")));
        }
        questions.insert(
            id.clone(),
            PendingQuestion {
                request: question,
                response,
            },
        );
    }
    let answer = tokio::select! {
        answer = received => answer.unwrap_or_else(|_| CreateElicitationResponse::new(ElicitationAction::Cancel)),
        _ = closed.changed() => CreateElicitationResponse::new(ElicitationAction::Cancel),
        _ = tokio::time::sleep(Duration::from_secs(30 * 60)) => CreateElicitationResponse::new(ElicitationAction::Cancel),
    };
    pending.lock().await.remove(&id);
    Ok(answer)
}

impl Connection {
    pub async fn questions(&self) -> Vec<QuestionRequest> {
        self.questions
            .lock()
            .await
            .values()
            .map(|pending| pending.request.clone())
            .collect()
    }

    pub async fn answer_question(&self, id: &str, answer: Answer) -> Result<(), String> {
        let mut pending = self.questions.lock().await;
        let question = pending
            .get(id)
            .ok_or("This agent interaction is no longer waiting.")?;
        let response = question.request.response(answer)?;
        let question = pending.remove(id).unwrap();
        question
            .response
            .send(response)
            .map_err(|_| "The agent disconnected before receiving the answer.".into())
    }
}
