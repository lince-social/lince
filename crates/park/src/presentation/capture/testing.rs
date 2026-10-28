use super::*;
use bevy::{pbr::ShadowView, render::RenderApp};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Resources {
    pub assets: [usize; 3],
    pub cameras: usize,
    pub shadow_views: usize,
    pub sessions: usize,
    pub indicators: usize,
    pub gpu: [[usize; 2]; 5],
    pub allocated_bytes: Option<u64>,
    pub descriptors: usize,
    pub retained_bytes: u64,
}

impl Presentation {
    pub(crate) fn capture_test_resources(&mut self) -> Resources {
        self.renderer.device.poll(wgpu::PollType::Wait {
            submission_index: None, timeout: Some(std::time::Duration::from_secs(3)),
        }).unwrap();
        let report = self.renderer._instance.generate_report().expect("The capture fixture has no real WGPU resource report");
        let gpu = [report.hub.textures, report.hub.texture_views, report.hub.buffers, report.hub.bind_groups, report.hub.samplers]
            .map(|resource| [resource.num_kept_from_user, resource.num_allocated]);
        let allocated_bytes = self.renderer.device.generate_allocator_report().map(|report| report.total_allocated_bytes);
        let world = self.app.world_mut();
        let assets = [world.resource::<Assets<Image>>().len(), world.resource::<Assets<Mesh>>().len(), world.resource::<Assets<StandardMaterial>>().len()];
        let cameras = world.query_filtered::<Entity, With<Camera>>().iter(world).count();
        let sessions = world.get_resource::<lince_interface::host::capture::Status>().map_or(0, |status| status.sessions.len());
        let indicators = world.query::<&Name>().iter(world).filter(|name| name.as_str() == "capture.active-indicator").count();
        let shadow_views = self.app.get_sub_app_mut(RenderApp).map_or(0, |render| {
            let world = render.world_mut();
            world.query_filtered::<Entity, With<ShadowView>>().iter(world).count()
        });
        Resources { assets, cameras, shadow_views, sessions, indicators, gpu, allocated_bytes, descriptors: std::fs::read_dir("/proc/self/fd").unwrap().count(),
            retained_bytes: self.renderer.capture_test_bytes() }
    }
}
