use super::*;
use lince_interface::host::display::{AdaptiveSyncStatus, AdaptiveSyncSupport};
use smithay::backend::drm::VrrSupport;

fn support(value: VrrSupport) -> AdaptiveSyncSupport {
    match value { VrrSupport::NotSupported => AdaptiveSyncSupport::Unsupported,
        VrrSupport::RequiresModeset => AdaptiveSyncSupport::RequiresModeset, VrrSupport::Supported => AdaptiveSyncSupport::Supported }
}

pub(super) fn prepare(screen: &mut Screen, planned: &PlannedOutput, strict: bool) -> io::Result<()> {
    let mut status = screen.adaptive_sync.clone();
    let capability = screen.output.with_compositor(|compositor| compositor.vrr_supported(planned.connector)).map_err(error);
    status.reason = None;
    status.support = match capability {
        Ok(value) => support(value),
        Err(failure) => {
            if strict && planned.adaptive_sync { return Err(failure); }
            status.reason = Some(format!("Adaptive-sync capability could not be read: {failure}"));
            AdaptiveSyncSupport::Unknown
        }
    };
    if planned.adaptive_sync && !status.support.available() {
        if strict { return Err(io::Error::other(format!("Adaptive sync is unavailable on {}", planned.monitor.name))); }
        status.reason.get_or_insert_with(|| "The saved adaptive-sync request is unavailable on this output; fixed refresh is retained.".into());
    } else if status.support == AdaptiveSyncSupport::Unsupported {
        status.reason = Some("The connector or DRM backend does not support adaptive sync.".into());
    }
    let desired = planned.adaptive_sync && status.support.available();
    let prepared = desired;
    if status.prepared != Some(prepared) || status.effective != Some(prepared) {
        screen.output.reset_buffers();
        status.pending = true;
    }
    status.prepared = Some(prepared);
    screen.adaptive_sync = status;
    Ok(())
}

fn observed(device: &impl control::Device, crtc: crtc::Handle) -> io::Result<Option<bool>> {
    let properties = device.get_properties(crtc)?;
    let (handles, values) = properties.as_props_and_values();
    for (handle, value) in handles.iter().zip(values) {
        let property = device.get_property(*handle)?;
        if property.name().to_bytes() == b"VRR_ENABLED" {
            return match *value { 0 => Ok(Some(false)), 1 => Ok(Some(true)), _ => Err(io::Error::other("The kernel returned an invalid adaptive-sync state")) };
        }
    }
    Ok(None)
}

pub(in super::super) fn observe_adaptive_sync(device: &impl control::Device, crtc: crtc::Handle, status: &mut AdaptiveSyncStatus, prepared: Option<bool>) {
    accept_observation(status, prepared, || observed(device, crtc));
}

fn accept_observation(status: &mut AdaptiveSyncStatus, prepared: Option<bool>, read: impl FnOnce() -> io::Result<Option<bool>>) {
    if !status.pending || prepared != status.prepared { return; }
    status.pending = false;
    match read() {
        Ok(effective) => {
            status.effective = effective;
            if effective.is_some() && effective != status.prepared {
                status.reason = Some("The kernel-observed adaptive-sync state differs from the prepared request.".into());
            } else if effective.is_none() && status.support.available() {
                status.reason = Some("The kernel does not expose an adaptive-sync state to confirm this request.".into());
            }
        }
        Err(failure) => {
            status.effective = None;
            status.reason = Some(format!("Adaptive-sync state could not be confirmed: {failure}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_matching_submitted_frame_may_publish_kernel_observed_state() {
        let mut state = AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
            prepared: Some(true), effective: Some(false), pending: true, reason: None };
        accept_observation(&mut state, Some(false), || panic!("A stale submitted frame must not read or acknowledge the new request"));
        assert!(state.pending);
        assert_eq!(state.effective, Some(false));
        accept_observation(&mut state, Some(true), || Ok(Some(true)));
        assert_eq!(state.effective, Some(true));
        assert!(!state.pending);
        assert!(state.reason.is_none());
        accept_observation(&mut state, Some(true), || panic!("An unchanged presented frame must not poll DRM repeatedly"));
    }

    #[test]
    fn unavailable_observation_is_unknown_and_refused_enable_is_not_reported_active() {
        for observed in [Ok(None), Err(io::Error::other("disconnected"))] {
            let mut state = AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
                prepared: Some(true), effective: Some(false), pending: true, reason: None };
            accept_observation(&mut state, Some(true), || observed);
            assert_eq!(state.effective, None);
            assert!(!state.pending);
            assert!(state.reason.is_some());
        }
        let mut state = AdaptiveSyncStatus { prepared: Some(true), pending: true, ..Default::default() };
        accept_observation(&mut state, Some(true), || Ok(Some(false)));
        assert_eq!(state.effective, Some(false));
        assert!(!state.pending);
        assert!(state.reason.as_deref().unwrap().contains("differs"));
    }
}

impl Hardware {
    pub(in super::super) fn publish_adaptive_sync(&self, server: &mut Server) {
        for head in &self.catalog {
            let status = self.screens.values().find(|screen| screen.id == head.id).map(|screen| screen.adaptive_sync.clone())
                .unwrap_or_else(|| AdaptiveSyncStatus { reason: Some("Enable this output to discover its adaptive-sync capability.".into()), ..Default::default() });
            server.output_adaptive_sync(head.id, status);
        }
    }

    pub(in super::super) fn test_adaptive_sync(&self, plan: &OutputPlan) -> io::Result<()> {
        for planned in plan.screens.iter().filter(|planned| planned.adaptive_sync) {
            let screen = self.screens.get(&planned.crtc).filter(|screen| screen.connector == planned.connector)
                .ok_or_else(|| io::Error::other(format!("Enable {} with fixed refresh before testing adaptive sync", planned.monitor.name)))?;
            let value = screen.output.with_compositor(|compositor| compositor.vrr_supported(planned.connector)).map_err(error)?;
            if !support(value).available() { return Err(io::Error::other(format!("Adaptive sync is unavailable on {}", planned.monitor.name))); }
        }
        Ok(())
    }
}
