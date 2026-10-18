use super::*;
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelListHandler, ForeignToplevelListState};

impl ForeignToplevelListHandler for State {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign
    }
}

impl State {
    pub(super) fn sync_foreign(&mut self) {
        self.sync_management();
        self.sync_workspaces();
        self.sync_output_control();
        self.foreign_handles.retain(|id, handle| {
            let keep = self.windows.get(id).is_some_and(|window| {
                window.mapped && window.role.wl_surface().is_alive()
            });
            if !keep {
                handle.send_closed();
            }
            keep
        });
        for (id, window) in &self.windows {
            if !window.mapped || !window.role.wl_surface().is_alive() {
                continue;
            }
            let (title, app_id) = with_states(window.role.wl_surface(), |states| {
                let metadata = states.data_map.get::<XdgToplevelSurfaceData>().unwrap()
                    .lock().unwrap_or_else(|error| error.into_inner());
                (bounded(metadata.title.as_deref().unwrap_or_default()), bounded(metadata.app_id.as_deref().unwrap_or_default()))
            });
            if let Some(handle) = self.foreign_handles.get(id) {
                if handle.title() != title || handle.app_id() != app_id {
                    handle.send_title(&title);
                    handle.send_app_id(&app_id);
                    handle.send_done();
                }
            } else {
                let handle = self.foreign.new_toplevel::<State>(title, app_id);
                self.foreign_handles.insert(*id, handle);
            }
        }
    }
}

smithay::delegate_foreign_toplevel_list!(State);
