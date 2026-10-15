use super::provider::Control;
use crate::EngineError;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct Request {
    pub owner: String,
    pub sources: Vec<String>,
    pub control: Control,
    pub started: std::time::Instant,
    pub state: State,
}

pub enum State {
    Running,
    Complete(Value),
    Failed(String),
    Cancelled,
}

#[derive(Default, Clone)]
pub struct Runtime {
    pub requests: Arc<Mutex<HashMap<String, Request>>>,
}

impl Runtime {
    pub fn start(
        &self,
        owner: &str,
        id: &str,
        sources: Vec<String>,
    ) -> Result<Control, EngineError> {
        if !nucleus::valid_uid(id, "kr") {
            return Err(EngineError::Consequence(
                "Choose a new knowledge request identity".into(),
            ));
        }
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| EngineError::Consequence("Knowledge requests unavailable".into()))?;
        if requests.contains_key(id) {
            return Err(EngineError::Consequence(
                "This knowledge request identity has already been used".into(),
            ));
        }
        if requests
            .values()
            .filter(|request| request.owner == owner && matches!(request.state, State::Running))
            .count()
            >= 2
            || requests
                .values()
                .filter(|request| matches!(request.state, State::Running))
                .count()
                >= 16
        {
            return Err(EngineError::Consequence(
                "Wait for a knowledge request to finish or cancel it".into(),
            ));
        }
        while requests.len() >= 32
            || requests
                .values()
                .filter(|request| request.owner == owner)
                .count()
                >= 8
        {
            let oldest = requests
                .iter()
                .filter(|(_, request)| !matches!(request.state, State::Running))
                .min_by_key(|(_, request)| request.started)
                .map(|(id, _)| id.clone());
            let Some(oldest) = oldest else {
                return Err(EngineError::Consequence(
                    "Knowledge request capacity reached".into(),
                ));
            };
            requests.remove(&oldest);
        }
        let control = Control::default();
        requests.insert(
            id.into(),
            Request {
                owner: owner.into(),
                sources,
                control: control.clone(),
                started: std::time::Instant::now(),
                state: State::Running,
            },
        );
        Ok(control)
    }

    pub fn finish(&self, id: &str, result: Result<Value, EngineError>) {
        if let Ok(mut requests) = self.requests.lock()
            && let Some(request) = requests.get_mut(id)
            && matches!(request.state, State::Running)
        {
            request.state = match result {
                Ok(data) => State::Complete(data),
                Err(error) => State::Failed(error.to_string()),
            };
        }
    }

    pub fn poll(&self, owner: &str, id: &str) -> Result<Value, EngineError> {
        let requests = self
            .requests
            .lock()
            .map_err(|_| EngineError::Consequence("Knowledge requests unavailable".into()))?;
        let request = requests
            .get(id)
            .filter(|request| request.owner == owner)
            .ok_or_else(|| EngineError::Consequence("Knowledge request unavailable".into()))?;
        Ok(match &request.state {
            State::Running => json!({"request_id":id,"status":"loading","complete":false}),
            State::Complete(data) => {
                json!({"request_id":id,"status":"complete","complete":true,"result":data})
            }
            State::Failed(message) => {
                json!({"request_id":id,"status":"failed","complete":true,"message":message})
            }
            State::Cancelled => json!({"request_id":id,"status":"cancelled","complete":true}),
        })
    }

    pub fn cancel(&self, owner: &str, id: &str) -> Result<(), EngineError> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| EngineError::Consequence("Knowledge requests unavailable".into()))?;
        let request = requests
            .get_mut(id)
            .filter(|request| request.owner == owner)
            .ok_or_else(|| EngineError::Consequence("Knowledge request unavailable".into()))?;
        if matches!(request.state, State::Running) {
            request.control.cancel();
            request.state = State::Cancelled;
        }
        Ok(())
    }

    pub fn clear(&self, owner: &str, source: Option<&str>) -> Result<(), EngineError> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| EngineError::Consequence("Knowledge requests unavailable".into()))?;
        for request in requests.values_mut().filter(|request| {
            request.owner == owner
                && source.is_none_or(|source| request.sources.iter().any(|known| known == source))
        }) {
            request.control.cancel();
            request.state = State::Cancelled;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_cannot_be_replaced_by_a_late_result_and_other_people_cannot_cancel() {
        let runtime = Runtime::default();
        let id = nucleus::new_uid("kr");
        let control = runtime.start("one", &id, vec!["test".into()]).unwrap();
        assert!(runtime.cancel("two", &id).is_err());
        runtime.cancel("one", &id).unwrap();
        assert!(control.checkpoint().is_err());
        runtime.finish(&id, Ok(json!({"item":"late"})));
        assert_eq!(runtime.poll("one", &id).unwrap()["status"], "cancelled");
        assert!(runtime.poll("two", &id).is_err());
    }
}
