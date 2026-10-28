use super::*;

pub(super) fn changed(previous: &[Entry], next: &[Entry]) -> u32 {
    [contract::CHANGE_WINDOW, contract::CHANGE_WORKSPACE, contract::CHANGE_OUTPUT, contract::CHANGE_CONTEXT, contract::CHANGE_COMMANDS]
        .into_iter().filter(|scope| previous.iter().filter(|entry| entry.scope() == *scope).ne(next.iter().filter(|entry| entry.scope() == *scope)))
        .fold(0, |scope, changed| scope | changed)
}

pub(super) fn page(entries: &[Entry], revision: u64, requested: u64, cursor: u32) -> Response {
    if requested != 0 && requested != revision {
        return Response::Failed { failure: Failure::new(Error::StaleTarget, "The desktop snapshot changed; restart from revision zero and cursor zero.") };
    }
    if requested == 0 && cursor != 0 || cursor as usize > entries.len() {
        return Response::Failed { failure: Failure::new(Error::InvalidArguments, "Use a current snapshot revision and its returned page cursor.") };
    }
    let mut page = Vec::new();
    let mut bytes = 1024;
    for entry in entries.iter().skip(cursor as usize) {
        let length = match serde_json::to_vec(entry) { Ok(encoded) => encoded.len() + 1, Err(_) => contract::RESPONSE_LIMIT };
        if bytes + length > contract::RESPONSE_LIMIT {
            if page.is_empty() { return Response::Failed { failure: Failure::new(Error::Unavailable, "This snapshot entry exceeds the bounded response size.") }; }
            break;
        }
        bytes += length;
        page.push(entry.clone());
    }
    let following = cursor as usize + page.len();
    Response::Snapshot { revision, next: (following < entries.len()).then_some(following as u32), entries: page }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_pages_preserve_every_unicode_entry_and_revision() {
        let entries = (1..=200).map(|id| Entry::Workspace { id, name: "界".repeat(300), active: id == 1, output: None }).collect::<Vec<_>>();
        let mut cursor = 0;
        let mut received = Vec::new();
        loop {
            let response = page(&entries, 17, if cursor == 0 { 0 } else { 17 }, cursor);
            assert!(serde_json::to_vec(&response).unwrap().len() <= contract::RESPONSE_LIMIT);
            let Response::Snapshot { revision, entries, next } = response else { panic!("A valid snapshot page was refused"); };
            assert_eq!(revision, 17);
            assert!(!entries.is_empty());
            received.extend(entries);
            let Some(next) = next else { break; };
            assert!(next > cursor);
            cursor = next;
        }
        assert_eq!(received, entries);
    }
}

impl State {
    pub(super) fn control_entries(&self) -> Vec<Entry> {
        let active = self.control.active.load(Ordering::Acquire);
        let eligible = active && self.lock.owner.is_none();
        let mut entries = vec![Entry::Session { locked: self.lock.owner.is_some(), active }, Entry::Context { value: eligible.then(|| self.control.catalog.context.clone()).flatten() }];
        if !eligible { return entries; }
        entries.extend(self.control.catalog.commands.iter().cloned().map(|mut value| {
            if self.control.input_owned.load(Ordering::Acquire) {
                value.unavailable = Some("Finish the current application popup or input-method interaction.".into());
            }
            Entry::Command { value }
        }));
        entries.extend(self.workspaces.snapshot_entries().map(|entry| {
            if let Entry::Workspace { id, name, active, .. } = entry {
                let output = self.presented_geometry.as_ref().and_then(|geometry| geometry.outputs.iter().find(|output| output.workspace == id)).map(|output| output.id.0);
                Entry::Workspace { id, name, active, output }
            } else { entry }
        }));
        for head in self.output_control.snapshot_heads() {
            let current = head.current.and_then(|index| head.modes.get(index));
            entries.push(Entry::Output { id: head.id.0, name: head.name.clone(), position: head.position,
                size: current.map_or([0, 0], |mode| mode.size.map(|size| size.max(0) as u32)), scale: head.scale,
                rotation: Some(head.rotation), adaptive_sync_requested: head.adaptive_sync,
                adaptive_sync: self.output_control.adaptive_sync.get(&head.id).cloned().unwrap_or_default(),
                enabled: current.is_some(), refresh_mhz: current.map(|mode| mode.refresh_mhz),
                modes: head.modes.iter().map(|mode| contract::Mode { size: mode.size, refresh_mhz: mode.refresh_mhz, preferred: mode.preferred }).collect(),
            });
        }
        for (id, registered) in &self.outputs {
            if self.output_control.snapshot_heads().iter().any(|head| head.id == *id) { continue; }
            let output = &registered.output;
            let origin = output.current_location();
            let size = output.current_mode().map_or([0, 0], |mode| [mode.size.w.max(0) as u32, mode.size.h.max(0) as u32]);
            entries.push(Entry::Output { id: id.0, name: output.name(), position: [origin.x, origin.y], size, scale: output.current_scale().fractional_scale(),
                rotation: crate::server::output_rotation(output.current_transform()), adaptive_sync_requested: false,
                adaptive_sync: self.output_control.adaptive_sync.get(id).cloned().unwrap_or_default(),
                enabled: output.current_mode().is_some(), refresh_mhz: output.current_mode().map(|mode| mode.refresh),
                modes: output.modes().into_iter().map(|mode| contract::Mode { size: [mode.size.w, mode.size.h], refresh_mhz: mode.refresh, preferred: output.preferred_mode() == Some(mode) }).collect(),
            });
        }
        for (id, window) in &self.windows {
            if !window.mapped || !window.role.wl_surface().is_alive() { continue; }
            let (title, app_id, parent) = with_states(window.role.wl_surface(), |states| {
                let metadata = states.data_map.get::<XdgToplevelSurfaceData>().unwrap().lock().unwrap_or_else(|error| error.into_inner());
                (bounded(metadata.title.as_deref().unwrap_or_default()), bounded(metadata.app_id.as_deref().unwrap_or_default()), metadata.parent.as_ref().and_then(|parent| self.id(parent)).map(|id| id.0))
            });
            entries.push(Entry::Window { surface: id.0, title, app_id, parent });
        }
        entries
    }
}
