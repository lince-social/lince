//! Local assistant discovery, handoff, and receipt inspection.

use std::{
    fmt::Display,
    sync::{PoisonError, atomic::Ordering::Acquire},
};

use engine::EngineError::{self, Consequence, Forbidden};
use fiote::assistant::Source;
use serde::Deserialize;
use serde_json::{Value, from_value, to_value};

use super::{Empty, Kind, NativeTool, error};

pub(super) const MAX_DISCOVERY_PAGE: usize = 30;

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
        let native = &self.native;
        let write = matches!(self.kind, Kind::Delegate);
        native
            .engine
            .access_scope(write, async {
                {
                    let state = native.state.lock().await;
                    if state.closed
                        || native.closed.load(Acquire)
                        || !state.session.local_authority()
                        || !state.session.subject_may_act().await
                    {
                        return Err(Forbidden(
                            "Native Fiote handoff requires an active local authorized session."
                                .into(),
                        ));
                    }
                }
                let service = native
                    .assistant
                    .as_ref()
                    .ok_or_else(|| consequence("Fiote service is unavailable."))?;
                let result = match self.kind {
                    Kind::Fiotes => {
                        let Discovery { offset, limit } =
                            from_value(arguments).map_err(consequence)?;
                        if !(1..=MAX_DISCOVERY_PAGE).contains(&limit) {
                            return Err(consequence(
                                "Discover between one and thirty Fiotes per page.",
                            ));
                        }
                        to_value(service.discover(offset, limit).await.map_err(Consequence)?)
                    }
                    Kind::Delegate => {
                        let input = from_value(arguments).map_err(consequence)?;
                        let message = native
                            .source_message
                            .read()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone()
                            .ok_or_else(|| {
                                consequence("This turn has no source Message for handoff.")
                            })?;
                        let source = Source {
                            fiote: native.context.record.clone(),
                            author: native.context.agent.clone(),
                            thread: native.context.thread.clone(),
                            message: message.to_string(),
                        };
                        to_value(service.delegate(source, input).await.map_err(Consequence)?)
                    }
                    Kind::Handoffs => {
                        from_value::<Empty>(arguments).map_err(consequence)?;
                        to_value(
                            service
                                .handoffs(&native.context.thread)
                                .await
                                .map_err(Consequence)?,
                        )
                    }
                    _ => return Err(consequence("Unknown assistant operation.")),
                };
                result.map_err(consequence)
            })
            .await
            .map_err(error)
    }
}

fn consequence(cause: impl Display) -> EngineError {
    Consequence(error(cause))
}
