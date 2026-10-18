use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Discovery {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

impl NativeTool {
    pub(super) async fn run_assistant(&self, arguments: Value) -> Result<Value, String> {
        let write = matches!(self.kind, Kind::Delegate);
        self.native
            .engine
            .access_scope(write, async {
                {
                    let state = self.native.state.lock().await;
                    if state.closed
                        || self.native.closed.load(Ordering::Acquire)
                        || !state.session.local_authority()
                        || !state.session.subject_may_act().await
                    {
                        return Err(engine::EngineError::Forbidden(
                            "Native Fiote handoff requires an active local authorized session."
                                .into(),
                        ));
                    }
                }
                let service = self.native.assistant.as_ref().ok_or_else(|| {
                    engine::EngineError::Consequence("Fiote service is unavailable.".into())
                })?;
                let result = match self.kind {
                    Kind::Fiotes => {
                        let input: Discovery = serde_json::from_value(arguments)
                            .map_err(|e| engine::EngineError::Consequence(error(e)))?;
                        if !(1..=30).contains(&input.limit) {
                            return Err(engine::EngineError::Consequence(
                                "Discover between one and thirty Fiotes per page.".into(),
                            ));
                        }
                        service
                            .discover(input.offset, input.limit)
                            .await
                            .and_then(|value| serde_json::to_value(value).map_err(error))
                    }
                    Kind::Delegate => {
                        let input = serde_json::from_value(arguments)
                            .map_err(|e| engine::EngineError::Consequence(error(e)))?;
                        let message = self
                            .native
                            .source_message
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .clone();
                        let source = fiote::assistant::Source {
                            fiote: self.native.context.record.clone(),
                            author: self.native.context.agent.clone(),
                            thread: self.native.context.thread.clone(),
                            message: message.ok_or_else(|| {
                                engine::EngineError::Consequence(
                                    "This turn has no source Message for handoff.".into(),
                                )
                            })?,
                        };
                        service
                            .delegate(source, input)
                            .await
                            .and_then(|value| serde_json::to_value(value).map_err(error))
                    }
                    Kind::Handoffs => {
                        serde_json::from_value::<Empty>(arguments)
                            .map_err(|e| engine::EngineError::Consequence(error(e)))?;
                        service
                            .handoffs(&self.native.context.thread)
                            .await
                            .and_then(|value| serde_json::to_value(value).map_err(error))
                    }
                    _ => Err("Unknown assistant operation.".into()),
                };
                result.map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(error)
    }
}
