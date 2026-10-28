use super::*;
use lince_interface::{host::{OutputId, OpeningRequest}, layout::desktop::PlacementId, native::layout::desktop::opening::{self, Decision}};
use nucleus::window_opening::Properties;

impl Presentation {
    fn opening_destination(&self, root: Entity, fallback: u64, properties: &Properties) -> (u64, Option<OutputId>, Option<String>) {
        let world = self.app.world();
        let spaces = world.get::<native::workspace::Workspaces>(root).unwrap();
        let mut warning = None;
        let requested = properties.workspace.filter(|requested| spaces.entries.iter().any(|space| space.id == *requested && !native::laboratory::temporary(world, root, *requested)));
        if properties.workspace.is_some() && requested.is_none() { warning = Some("The requested workspace is unavailable; the retained or current workspace was used.".into()); }
        let output = properties.output.as_ref().and_then(|name| self.output_state.outputs.iter().find(|output| &output.name == name)).map(|output| output.viewport.id);
        if properties.output.is_some() && output.is_none() { warning = Some("The requested output is disconnected or unavailable; the retained or current output was used.".into()); }
        let destination = requested.or_else(|| {
            properties.workspace.is_none().then(|| output.and_then(|id| self.output_state.assignments.outputs().iter().find(|output| output.id == id).map(|output| output.active))).flatten()
        }).unwrap_or(fallback);
        let output = if let Some(visible) = self.output_state.assignments.outputs().iter().find(|output| output.active == destination)
            && output.is_some_and(|requested| requested != visible.id)
        {
            warning = Some("The requested workspace is already visible on another output; its existing view and input owner were preserved.".into());
            Some(visible.id)
        } else { output };
        (destination, output, warning)
    }

    pub(super) fn opening_window(&mut self, root: Entity, entity: Entity, identity: PlacementId, title: &str, app_id: &str) -> Result<(), ProbeFailure> {
        let profile = self.records.as_ref().and_then(|records| records.profile(identity));
        let uid = profile.and_then(|profile| profile.uid.as_deref());
        let mut metadata = self.app.world().get::<native::area::RecordProperties>(entity)
            .map(|properties| properties.0.clone()).filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
        metadata["head"] = serde_json::json!(title);
        metadata["app_id"] = serde_json::json!(app_id);
        metadata["uid"] = serde_json::json!(uid);
        let mut decision = opening::capture(self.app.world(), &metadata, uid);
        decision.retained = profile.is_some();
        let fallback = self.app.world().get::<native::workspace::WorkspaceMember>(entity).unwrap().0;
        let (workspace, _, warning) = self.opening_destination(root, fallback, &decision.properties);
        if let Some(warning) = warning { decision.warning = Some(warning); }
        self.app.world_mut().entity_mut(entity).insert((decision, native::workspace::WorkspaceMember(workspace)));
        Ok(())
    }

    pub(crate) fn initial_activate_application(&mut self, surface: lince_interface::host::SurfaceId) -> Result<(), ProbeFailure> {
        if self.windows.get(&surface).is_some_and(|window| {
            let world = self.app.world();
            world.get::<native::layout::desktop::TransientParent>(window.entity).is_none()
                && world.get::<Decision>(window.entity).is_some_and(|decision| decision.properties.focus == Some(false))
        }) {
            return Ok(());
        }
        let output = if let Some(window) = self.windows.get(&surface) {
            let world = self.app.world();
            let properties = world.get::<Decision>(window.entity).map(|decision| decision.properties.clone()).unwrap_or_default();
            let workspace = world.get::<native::workspace::WorkspaceMember>(window.entity).map(|member| member.0);
            if let Some(workspace) = workspace {
                let root = self.canvas_root()?;
                self.opening_destination(root, workspace, &properties).1
            } else { None }
        } else { None };
        self.activate_application_on(surface, output)
    }

    pub(crate) fn reapply_opening(&mut self, request: OpeningRequest) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        if let Err(error) = self.try_reapply_opening(request) { native::notifications::report(self.app.world(), "Opening rules", &error.to_string()); }
        Ok(())
    }

    pub(super) fn try_reapply_opening(&mut self, request: OpeningRequest) -> Result<(), ProbeFailure> {
        if self.suspended() { return Err(input_failure("The Park view is inactive.")); }
        let result = (|| {
            let root = self.canvas_root()?;
            if !opening::request_current(self.app.world(), root, &request) { return Err(input_failure("Opening preferences or the captured workspace/output view changed after this command was requested; review and Reapply again.")); }
            let target = request.target;
            if !target.valid() || root.to_bits() != target.owner { return Err(input_failure("The captured canvas is unavailable.")); }
            let window = self.windows.get(&target.surface).filter(|_| self.identities.get(&target.surface) == Some(&PlacementId(target.placement)))
                .ok_or_else(|| input_failure("The captured application is no longer available."))?;
            let entity = window.entity;
            let world = self.app.world();
            if world.get::<native::workspace::WorkspaceMember>(entity).is_none_or(|member| member.0 != target.workspace)
                || world.get::<native::layout::desktop::TransientParent>(entity).is_some()
            { return Err(input_failure("The captured application changed workspace or ownership.")); }
            let profile = self.records.as_ref().and_then(|records| records.profile(PlacementId(target.placement)));
            let uid = profile.and_then(|profile| profile.uid.as_deref());
            let properties = world.get::<native::area::RecordProperties>(entity).map(|properties| &properties.0);
            let title = properties.and_then(|properties| properties.get("window_title").or_else(|| properties.get("head"))).cloned().unwrap_or_default();
            let app_id = properties.and_then(|properties| properties.get("app_id")).cloned().unwrap_or_default();
            let mut metadata = properties.cloned().filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
            metadata["head"] = title;
            metadata["app_id"] = app_id;
            metadata["uid"] = serde_json::json!(uid);
            let mut decision = opening::capture(world, &metadata, uid);
            if let Some(warning) = decision.warning.clone() { return Err(input_failure(&warning)); }
            decision.retained = true;
            let (destination, output, warning) = self.opening_destination(root, target.workspace, &decision.properties);
            if let Some(warning) = warning { return Err(input_failure(&warning)); }
            let focus = decision.properties.focus == Some(true);
            let mut assignments = self.output_state.assignments.clone();
            if focus && !self.output_state.outputs.is_empty() {
                let output = output.or_else(|| assignments.outputs().iter().find(|output| output.active == destination).map(|output| output.id)).or(assignments.focused())
                    .ok_or_else(|| input_failure("The destination output is unavailable."))?;
                assignments.select(output, destination).and_then(|_| assignments.focus(output))
                    .map_err(|_| input_failure("The destination output cannot select this workspace."))?;
            }
            self.input_window()?;
            let prepared = opening::prepare_reapply(self.app.world_mut(), root, target.workspace, PlacementId(target.placement), destination, &decision).map_err(|error| input_failure(&error))?;
            let selection = prepared.commit(self.app.world_mut()).map_err(|error| input_failure(&error))?;
            self.app.world_mut().entity_mut(entity).insert(decision);
            if focus {
                self.application_focus = None;
                if !self.output_state.outputs.is_empty() {
                    self.output_state.assignments = assignments;
                    self.select_output_workspace()?;
                    self.refresh_outputs()?;
                }
                if let Some(selection) = selection { selection.activate(self.app.world_mut()); }
            }
            native::notifications::report(self.app.world(), "Opening rules", "Opening preferences reapplied to the captured application.");
            Ok::<(), ProbeFailure>(())
        })();
        result
    }
}
