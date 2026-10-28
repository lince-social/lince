use super::*;

#[test]
#[ignore = "requires an explicitly selected primary DRM device and permission to create/read kernel property blobs"]
fn twenty_real_kernel_blob_lifetimes_retain_submitted_formats_and_reject_old_wake_and_withdrawn_color_generations() {
    use std::os::fd::OwnedFd;
    let path = std::env::var_os("LINCE_PARK_TEST_DRM_DEVICE").expect("Select the approved primary DRM device explicitly");
    let file = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
    let device = DrmDeviceFd::new(smithay::utils::DeviceFd::from(OwnedFd::from(file)));
    smithay::reexports::drm::Device::set_client_capability(&device, smithay::reexports::drm::ClientCapability::UniversalPlanes, true).unwrap();
    let resources = device.resource_handles().unwrap();
    let (connector, mode) = resources.connectors().iter().find_map(|connector| {
        let info = device.get_connector(*connector, false).ok()?;
        info.modes().first().copied().map(|mode| (*connector, mode))
    }).expect("The selected primary device must expose a connector mode");
    let descriptors = std::fs::read_dir("/proc/self/fd").unwrap().count();
    for cycle in 0..20 {
        let output = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
        let bytes = metadata(output);
        let property = device.create_property_blob(&bytes).unwrap();
        let id = property.as_blob().expect("The kernel did not return a property blob");
        assert_eq!(device.get_property_blob(id).unwrap(), bytes);
        let pipeline = Arc::new(Pipeline { output, mode, format: Fourcc::Abgr2101010, modifiers: Vec::new(),
            properties: vec![(connector, "HDR_OUTPUT_METADATA", id)], _blob: Some(Blob { device: device.clone(), id }), reported: false });
        let weak = Arc::downgrade(&pipeline);
        let mut state = State { prepared: Some(pipeline.clone()), status: ColorStatus { hdr10: true, prepared: Some(output),
            effective: Some(OutputColor::default()), revision: cycle * 4 + 1, pending: true, ..Default::default() }, ..Default::default() };
        let before_wake = state.submitted();
        state.sleep();
        assert!(!state.accept_observation(&before_wake, || panic!("A sleeping generation performed a kernel confirmation read")));
        state.wake().unwrap();
        let waking = state.submitted();
        assert!(!state.accept_observation(&before_wake, || panic!("A pre-wake frame confirmed the new color generation")));
        assert!(state.status.pending);
        let planes = device.plane_handles().unwrap();
        let plane = planes.into_iter().find(|plane| device.get_plane(*plane).is_ok_and(|plane| plane.framebuffer().is_some()))
            .expect("The selected device must have an actual active framebuffer for the negative color observation");
        assert!(state.accept_observation(&waking, || observed(&device, connector, plane, &waking.pipeline)),
            "Uninstalled HDR metadata was incorrectly confirmed against the actual kernel state");
        assert!(state.status.effective.is_none());
        assert!(!state.status.pending);
        assert!(state.failure.is_some());
        state.wake().unwrap();
        let withdrawn = state.submitted();
        state.invalidate();
        assert!(!state.accept_observation(&withdrawn, || panic!("An inactive/withdrawn output performed a confirmation read")));
        state.prepared = None;
        drop(pipeline);
        assert!(weak.upgrade().is_some());
        assert_eq!(device.get_property_blob(id).unwrap(), bytes, "The submitted frame lost its kernel metadata owner");
        assert_eq!(waking.pipeline.format, Fourcc::Abgr2101010);
        drop(before_wake); drop(waking);
        assert_eq!(device.get_property_blob(id).unwrap(), bytes);
        drop(withdrawn); drop(state);
        assert!(weak.upgrade().is_none());
        assert!(device.get_property_blob(id).is_err(), "The final submitted owner leaked a kernel property blob");
        assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), descriptors, "Settled color lifetimes retained descriptors");
    }
}
