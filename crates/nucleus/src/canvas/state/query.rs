use super::State;
use crate::canvas::{Component, Document, Placement, Response, Snapshot, authoring};

impl State {
    pub(super) fn inspect_element(&self, placement: &str) -> Result<Response, String> {
        Ok(Response::Element {
            element: authoring::Element {
                revision: self.snapshot.revision,
                placement: self.placement(placement)?.clone(),
                parts: Vec::new(),
                total_parts: 0,
                next_offset: None,
                capabilities: [
                    "placement-crud",
                    "authored-parts",
                    "tokens",
                    "event-links",
                    "event-action-behaviors",
                    "groups",
                    "export",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            },
        })
    }

    pub(super) fn export(&self, placement: &str, name: &str) -> Result<Response, String> {
        let placement = self.placement(placement)?;
        let mut edits = authoring::edits(&placement.component);
        edits.size = Some(placement.geometry.size);
        if let Some(root) = edits.parts.get_mut("root") {
            root.geometry = None;
        }
        let document = Document {
            format: Document::FORMAT.into(),
            name: name.into(),
            component: Component::Authored {
                base: Box::new(authoring::base(&placement.component).clone()),
                edits,
            },
        };
        let hash = authoring::hash(&document)?;
        Ok(Response::Export {
            document,
            hash,
            revision: self.snapshot.revision,
        })
    }

    pub(super) fn inspect(
        &self,
        workspace: Option<u64>,
        selected_only: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Response, String> {
        if workspace.is_some_and(|id| !self.snapshot.workspaces.iter().any(|w| w.id == id)) {
            return Err("The workspace no longer exists.".into());
        }
        let mut placements = self
            .snapshot
            .placements
            .iter()
            .filter(|placement| {
                workspace.is_none_or(|id| placement.workspace == id)
                    && (!selected_only || placement.selected)
            })
            .skip(offset);
        let page = placements.by_ref().take(limit).cloned().collect();
        let snapshot = Snapshot {
            revision: self.snapshot.revision,
            active_workspace: self.snapshot.active_workspace,
            workspaces: self.snapshot.workspaces.clone(),
            placements: page,
            next_offset: placements.next().map(|_| offset + limit),
        };
        snapshot.validate()?;
        Ok(Response::Snapshot { snapshot })
    }

    fn placement(&self, id: &str) -> Result<&Placement, String> {
        self.snapshot
            .placements
            .iter()
            .find(|placement| placement.id == id)
            .ok_or_else(|| "The placement no longer exists.".into())
    }
}
