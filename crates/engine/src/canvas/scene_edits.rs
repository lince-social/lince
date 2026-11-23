//! Acceptance for a locally opened model. The Canvas broker supplies the live caller and
//! Fiote provenance. The host owns the open directory and its unsaved draft.
use nucleus::{
    canvas::scene::{
        DocumentId, Scene,
        operation::{Edit, Receipt},
    },
    component::composition::Origin,
};
use std::{io, path::Path};
use store::workspace_scene::acceptance::{self, Accepted};

pub struct LocalScene<'a> {
    pub directory: &'a Path,
    pub document: &'a DocumentId,
    pub dirty: bool,
}

pub enum Outcome {
    Published(Accepted),
    RecoveredWithDraft(Receipt),
}

impl LocalScene<'_> {
    pub fn submit(
        &self,
        edit: &Edit,
        origin: Option<&Origin>,
        cancelled: impl Fn() -> bool,
        projection: impl FnOnce(&Scene) -> Result<(), String>,
    ) -> Result<Outcome, String> {
        if self.document != &edit.document {
            return Err("The captured document is outside this open workspace.".into());
        }
        edit.validate()?;
        if self.dirty {
            if cancelled() {
                return Err("Scene request cancelled.".into());
            }
            return match acceptance::receipt(self.directory, self.document, Some(&edit.request_id))
            {
                Ok(receipt) if &receipt.edit == edit && receipt.fiote.as_ref() == origin => {
                    Ok(Outcome::RecoveredWithDraft(receipt))
                }
                _ => Err(
                    "Save the native draft or reopen the accepted version before submitting."
                        .into(),
                ),
            };
        }
        let result = acceptance::accept(self.directory, edit, origin, |next| {
            projection(next).map_err(io::Error::other)?;
            if cancelled() {
                return Err(io::Error::other(
                    "Scene request cancelled before publication.",
                ));
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        tracing::debug!(target: "engine::scene_edits", commit_us = result.processing.as_micros(), staged_bytes = result.writes.staged_bytes, peak_file_bytes = result.writes.largest_staged_file_bytes, replayed = result.replayed, "Scene request resolved");
        Ok(Outcome::Published(result))
    }
}
