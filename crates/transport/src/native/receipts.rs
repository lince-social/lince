//! Connection-local replay receipts and pending proposal previews.

use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use nucleus::json::{encoded_len, sha256};
use serde_json::Value;
use tokio::{runtime::Handle, sync::Mutex};

use super::{MAX_RESULT_BYTES, State, error};

const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_OPERATIONS: usize = 4096;
const MAX_RECEIPT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREVIEWS: usize = 4;
const INTERRUPTED_PREVIEW_BYTES: usize = 160;

type Fingerprint = [u8; 32];
type Outcome = Result<Value, String>;

struct Receipt {
    fingerprint: Fingerprint,
    result: Outcome,
}

#[derive(Default)]
pub(super) struct Receipts {
    completed: BTreeMap<String, Receipt>,
    pending: BTreeMap<String, Fingerprint>,
    bytes: usize,
}

impl Receipts {
    pub(super) fn previous(&self, id: &str, args: &Value) -> Result<Option<Value>, String> {
        if id.is_empty()
            || id.len() > MAX_REQUEST_ID_BYTES
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(
                "Use a request_id of 1–128 letters, digits, hyphens or underscores.".into(),
            );
        }
        if let Some(original) = self.pending.get(id) {
            return Err(if *original == fingerprint(args)? {
                "This preview is still running. Retry the same request after it finishes."
            } else {
                "This request_id was already used for a different operation."
            }
            .into());
        }
        if let Some(receipt) = self.completed.get(id) {
            if receipt.fingerprint != fingerprint(args)? {
                return Err("This request_id was already used for a different operation.".into());
            }
            return receipt.result.clone().map(Some);
        }
        if self.operations() >= MAX_OPERATIONS || self.bytes >= MAX_RECEIPT_BYTES {
            return Err("This connection has reached its operation budget. Close it and open a new connection. Inspect uncertain effects before retrying.".into());
        }
        Ok(None)
    }

    pub(super) fn remember(&mut self, id: String, fingerprint: Fingerprint, result: Outcome) {
        let payload_bytes = match &result {
            Ok(value) => encoded_len(value).unwrap_or(MAX_RESULT_BYTES),
            Err(message) => message.len(),
        };
        self.bytes += id.len() + fingerprint.len() + payload_bytes;
        self.completed.insert(
            id,
            Receipt {
                fingerprint,
                result,
            },
        );
    }

    pub(super) fn begin_preview(&mut self, id: &str, args: &Value) -> Result<Fingerprint, String> {
        if self.pending.len() >= MAX_PREVIEWS || self.operations() >= MAX_OPERATIONS {
            return Err(
                "This connection's preview budget is occupied. Retry after a preview finishes."
                    .into(),
            );
        }
        let mark = fingerprint(args)?;
        self.pending.insert(id.into(), mark);
        Ok(mark)
    }

    pub(super) fn end_preview(&mut self, id: &str) {
        self.pending.remove(id);
    }

    fn interrupt(&mut self, id: String, fingerprint: Fingerprint) {
        if self.pending.get(&id) == Some(&fingerprint) {
            self.pending.remove(&id);
            self.bytes += id.len() + INTERRUPTED_PREVIEW_BYTES;
            self.completed.insert(
                id,
                Receipt {
                    fingerprint,
                    result: Err("This preview was interrupted. Request a fresh preview.".into()),
                },
            );
        }
    }

    fn operations(&self) -> usize {
        self.completed.len() + self.pending.len()
    }
}

pub(super) fn fingerprint(arguments: &Value) -> Result<Fingerprint, String> {
    let mut canonical = arguments.clone();
    canonical.sort_all_objects();
    sha256(&canonical).map_err(error)
}

pub(super) struct PreviewPermit {
    state: Weak<Mutex<State>>,
    request: Option<String>,
    fingerprint: Fingerprint,
}

impl PreviewPermit {
    pub(super) fn new(
        state: &Arc<Mutex<State>>,
        request: String,
        fingerprint: Fingerprint,
    ) -> Self {
        Self {
            state: Arc::downgrade(state),
            request: Some(request),
            fingerprint,
        }
    }

    pub(super) fn finish(mut self) {
        self.request = None;
    }
}

impl Drop for PreviewPermit {
    fn drop(&mut self) {
        let Some(request) = self.request.take() else {
            return;
        };
        let (Some(state), Ok(runtime)) = (self.state.upgrade(), Handle::try_current()) else {
            return;
        };
        let fingerprint = self.fingerprint;
        runtime.spawn(async move {
            state.lock().await.receipts.interrupt(request, fingerprint);
        });
    }
}
