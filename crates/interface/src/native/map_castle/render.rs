use super::*;
pub(super) use crate::map_view::{Scenes, Target};

pub(super) fn install(app: &mut App) {
    app.init_resource::<diagnostics::Diagnostics>()
        .add_plugins(crate::map_view::MapRendererPlugin);
}
