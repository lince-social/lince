// non-lean-yet
// Scope: struct Completion, fn install.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 18.

use std::task::{Wake, Waker};

use bevy::render::render_resource::PipelineCache;

use super::*;

struct Completion(WakeSignal);

impl Wake for Completion {
    fn wake(self: Arc<Self>) {
        self.0.ring();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.ring();
    }
}

pub(super) fn install(app: &mut App) {
    let wake = app.world().resource::<WakeSignal>().clone();
    app.sub_app_mut(RenderApp)
        .world_mut()
        .resource_mut::<PipelineCache>()
        .set_completion_waker(Waker::from(Arc::new(Completion(wake))));
}

impl Presentation {
    pub(super) fn pipelines_pending(&self) -> bool {
        self.app
            .sub_app(RenderApp)
            .world()
            .resource::<PipelineCache>()
            .compilation_pending()
    }

    pub(super) fn complete_pipelines(&mut self) {
        self.app
            .sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<PipelineCache>()
            .block_on_compilation();
    }
}
