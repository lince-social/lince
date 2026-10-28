use super::*;
use lince_interface::host::{color::{ColorMode, OutputColor}, display::ColorStatus};
use smithay::{backend::allocator::Modifier, reexports::drm::control::{plane, property}};

mod display_info;
#[cfg(test)]
mod lifetimes;

struct Blob { device: DrmDeviceFd, id: u64 }

impl Drop for Blob {
    fn drop(&mut self) { let _ = self.device.destroy_property_blob(self.id); }
}

pub(in super::super) struct Pipeline {
    pub output: OutputColor,
    mode: control::Mode,
    format: Fourcc,
    modifiers: Vec<Modifier>,
    properties: Vec<(connector::Handle, &'static str, u64)>,
    _blob: Option<Blob>,
    reported: bool,
}

impl Pipeline {
    pub(super) fn restores(&self, mode: control::Mode, connector: connector::Handle) -> bool {
        self.mode == mode && self.properties.iter().all(|(owner, _, _)| *owner == connector)
    }
}

#[derive(Clone)]
pub(in super::super) struct Submitted {
    revision: u64,
    pipeline: Arc<Pipeline>,
}

#[derive(Default)]
pub(in super::super) struct State {
    pub status: ColorStatus,
    baseline: Option<(Fourcc, Vec<(connector::Handle, &'static str, u64)>)>,
    pub prepared: Option<Arc<Pipeline>>,
    active: Option<Arc<Pipeline>>,
    hdr: Option<Arc<Pipeline>>,
    failure: Option<String>,
    power_reason: Option<Option<String>>,
}

impl State {
    pub fn sleep(&mut self) {
        if self.power_reason.is_none() { self.power_reason = Some(self.status.reason.take()); }
        self.status.pending = false;
        self.status.reason = Some("The output is sleeping; its last confirmed color profile is retained until wake.".into());
    }

    pub fn wake(&mut self) -> io::Result<()> {
        self.status.revision = self.status.revision.checked_add(1).ok_or_else(|| io::Error::other("Output color generation exhausted."))?;
        if self.power_reason.is_none() { self.power_reason = Some(self.status.reason.take()); }
        self.status.pending = true;
        self.status.reason = Some("Waiting for a matching output frame after power-on; the last confirmed color is retained.".into());
        Ok(())
    }

    pub fn submitted(&self) -> Submitted {
        Submitted { revision: self.status.revision, pipeline: self.prepared.as_ref().unwrap().clone() }
    }

    pub fn output(&self) -> OutputColor { self.prepared.as_ref().map_or(Default::default(), |pipeline| pipeline.output) }

    pub fn format(&self) -> Fourcc { self.prepared.as_ref().map_or(Fourcc::Argb8888, |pipeline| pipeline.format) }

    pub fn invalidate(&mut self) {
        self.power_reason = None;
        self.status.revision = self.status.revision.saturating_add(1);
        self.status.effective = None;
        self.status.pending = false;
        self.status.reason = Some("The physical session is inactive; output color will be confirmed after resume.".into());
        self.active = None;
        self.failure = None;
    }

    fn matches_submission(&self, submitted: &Submitted) -> bool {
        self.status.pending && submitted.revision == self.status.revision
            && self.prepared.as_ref().is_some_and(|prepared| Arc::ptr_eq(prepared, &submitted.pipeline))
    }

    fn accept_observation(&mut self, submitted: &Submitted, read: impl FnOnce() -> io::Result<()>) -> bool {
        if !self.matches_submission(submitted) { return false; }
        let result = read();
        self.status.pending = false;
        match result {
            Ok(()) => {
                self.status.effective = Some(submitted.pipeline.output);
                self.active = Some(submitted.pipeline.clone());
                if let Some(reason) = self.power_reason.take() { self.status.reason = reason; }
                false
            }
            Err(error) => {
                self.power_reason = None;
                let reason = format!("Output color could not be confirmed: {error}");
                self.status.effective = None;
                self.status.reason = Some(reason.clone());
                let fallback = submitted.pipeline.output.mode == ColorMode::Hdr10;
                if fallback { self.failure = Some(reason); }
                fallback
            }
        }
    }
}

struct Connector {
    sdr: Vec<(connector::Handle, &'static str, u64)>,
    hdr: Option<(u64, u64)>,
    edid: Option<Vec<u8>>,
}

fn enum_value(value: property::ValueType, name: &[u8]) -> Option<u64> {
    let property::ValueType::Enum(values) = value else { return None };
    values.values().1.iter().find(|value| value.name().to_bytes() == name).map(|value| value.value())
}

fn connector_state(device: &impl control::Device, connector: connector::Handle) -> io::Result<Connector> {
    let properties = device.get_properties(connector)?;
    let mut sdr = Vec::new();
    let mut hdr = false;
    let mut colorspace = None;
    let mut bpc = None;
    let mut edid = None;
    for (handle, value) in properties {
        let info = device.get_property(handle)?;
        match info.name().to_bytes() {
            b"EDID" if value != 0 => { edid = device.get_property_blob(value).ok(); }
            b"HDR_OUTPUT_METADATA" if info.mutable() && matches!(info.value_type(), property::ValueType::Blob) => {
                sdr.push((connector, "HDR_OUTPUT_METADATA", 0));
                hdr = true;
            }
            b"Colorspace" if info.mutable() => {
                if let Some(default) = enum_value(info.value_type(), b"Default") {
                    sdr.push((connector, "Colorspace", default));
                    colorspace = enum_value(info.value_type(), b"BT2020_RGB");
                }
            }
            b"max bpc" if info.mutable() => {
                if let property::ValueType::UnsignedRange(minimum, maximum) = info.value_type() && minimum <= maximum {
                    sdr.push((connector, "max bpc", value.clamp(minimum, maximum)));
                    if minimum <= 10 && maximum >= 10 { bpc = Some(10); }
                }
            }
            _ => {}
        }
    }
    Ok(Connector { sdr, hdr: hdr.then(|| colorspace.zip(bpc)).flatten(), edid })
}

fn metadata(output: OutputColor) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[4] = 2;
    let primaries = lince_interface::host::color::Primaries::Bt2020.chromaticities();
    for (index, coordinate) in primaries.into_iter().flatten().enumerate() {
        bytes[6 + index * 2..8 + index * 2].copy_from_slice(&(((coordinate + 10) / 20) as u16).to_ne_bytes());
    }
    bytes[22..24].copy_from_slice(&(output.peak_nits as u16).to_ne_bytes());
    bytes[24..26].copy_from_slice(&50_u16.to_ne_bytes());
    bytes
}

fn modifiers(renderer: &GlesRenderer, format: Fourcc) -> Vec<Modifier> {
    renderer.egl_context().dmabuf_render_formats().iter()
        .filter(|candidate| candidate.code == format).map(|candidate| candidate.modifier).collect()
}

fn candidate_hdr(
    outputs: &Outputs,
    renderer: &mut GlesRenderer,
    screen: &Screen,
    planned: &PlannedOutput,
    connector: &Connector,
    ready: Option<bool>,
) -> io::Result<Arc<Pipeline>> {
    match ready {
        None => return Err(io::Error::other("Waiting for the first native frame to confirm high-precision rendering.")),
        Some(false) => return Err(io::Error::other("The current native/graphics renderer cannot preserve HDR precision.")),
        Some(true) => {}
    }
    if let Some(reason) = &screen.color.failure { return Err(io::Error::other(reason.clone())); }
    if !outputs.device().is_atomic() { return Err(io::Error::other("HDR output requires an atomic DRM backend.")); }
    let (colorspace, bpc) = connector.hdr.ok_or_else(|| io::Error::other("The connector does not expose the required HDR metadata, BT.2020 RGB and ten-bit controls."))?;
    let hdr = display_info::hdr(connector.edid.as_deref().ok_or_else(|| io::Error::other("The connector has no readable display information."))?)?;
    let output = OutputColor { mode: ColorMode::Hdr10, peak_nits: hdr.peak_nits };
    let values = |id| vec![(planned.connector, "HDR_OUTPUT_METADATA", id),
        (planned.connector, "Colorspace", colorspace), (planned.connector, "max bpc", bpc)];
    if let Some(previous) = screen.color.hdr.as_ref().filter(|previous| {
        previous.output == output && previous.reported == hdr.reported && previous.mode == planned.mode
            && previous.properties == values(previous._blob.as_ref().unwrap().id)
    }) {
        screen.output.with_compositor(|compositor| compositor.test_color_format(outputs.allocator().clone(), planned.mode,
            previous.format, previous.modifiers.clone(), &previous.properties, screen.adaptive_sync.prepared == Some(true))).map_err(error)?;
        return Ok(previous.clone());
    }
    let property = outputs.device().create_property_blob(&metadata(output))?;
    let blob = Blob { device: outputs.device().device_fd().clone(), id: property.as_blob().ok_or_else(|| io::Error::other("The HDR metadata allocation did not return a blob."))? };
    let properties = values(blob.id);
    let mut failure = "No common ten-bit renderer/scanout format is available.".to_owned();
    for format in [Fourcc::Abgr2101010, Fourcc::Argb2101010] {
        let modifiers = modifiers(renderer, format);
        if modifiers.is_empty() { continue; }
        let test = (|| {
            let mut texture: GlesTexture = renderer.create_buffer(format, (1, 1).into()).map_err(error)?;
            drop(renderer.bind(&mut texture).map_err(error)?);
            screen.output.with_compositor(|compositor| compositor.test_color_format(outputs.allocator().clone(), planned.mode,
                format, modifiers.clone(), &properties, screen.adaptive_sync.prepared == Some(true))).map_err(error)
        })();
        match test {
            Ok(()) => return Ok(Arc::new(Pipeline { output, mode: planned.mode, format, modifiers,
                properties, _blob: Some(blob), reported: hdr.reported })),
            Err(error) => failure = error.to_string(),
        }
    }
    Err(io::Error::other(format!("HDR10 candidate test failed: {failure}")))
}

pub(super) fn prepare(outputs: &Outputs, renderer: &mut GlesRenderer, screen: &mut Screen, planned: &PlannedOutput, ready: Option<bool>, strict: bool) -> io::Result<()> {
    if strict && planned.color == ColorMode::Srgb { screen.color.failure = None; }
    let connector = connector_state(outputs.device(), planned.connector)?;
    if screen.color.baseline.is_none() {
        screen.color.baseline = Some((screen.output.format(), connector.sdr.clone()));
    }
    let hdr = candidate_hdr(outputs, renderer, screen, planned, &connector, ready);
    let (supported, reason) = match &hdr { Ok(_) => (true, None), Err(failure) => (false, Some(failure.to_string())) };
    if strict && planned.color == ColorMode::Hdr10 && !supported { return Err(io::Error::other(reason.unwrap())); }
    if let Ok(hdr) = hdr { screen.color.hdr = Some(hdr); }
    let mut prepared = if let Some(restore) = &planned.color_restore { restore.clone() }
        else if planned.color == ColorMode::Hdr10 && supported { screen.color.hdr.as_ref().unwrap().clone() }
        else {
            let (format, properties) = screen.color.baseline.as_ref().unwrap();
            Arc::new(Pipeline { output: Default::default(), mode: planned.mode, format: *format,
                modifiers: modifiers(renderer, *format), properties: properties.clone(), _blob: None, reported: false })
        };
    let changed = screen.color.prepared.as_ref().is_none_or(|previous| {
        previous.output != prepared.output || previous.mode != prepared.mode || previous.format != prepared.format || previous.properties != prepared.properties
    });
    if !changed { prepared = screen.color.prepared.as_ref().unwrap().clone(); }
    let vrr = screen.adaptive_sync.prepared == Some(true);
    let pending_matches = screen.output.with_compositor(|compositor| {
        let properties = compositor.surface().pending_connector_properties();
        compositor.vrr_enabled() == vrr && compositor.format() == prepared.format && compositor.surface().pending_mode() == planned.mode
            && properties.len() == prepared.properties.len() && prepared.properties.iter().all(|expected| properties.contains(expected))
    });
    if changed || screen.color.status.effective.is_none() || !pending_matches {
        let revision = screen.color.status.revision.checked_add(1).ok_or_else(|| io::Error::other("Output color revision exhausted."))?;
        let result = screen.output.with_compositor(|compositor| compositor.use_color_format(outputs.allocator().clone(), planned.mode,
            prepared.format, prepared.modifiers.clone(), &prepared.properties, vrr)).map_err(error);
        if let Err(failure) = result {
            if strict || !vrr { return Err(failure); }
            screen.output.with_compositor(|compositor| compositor.use_color_format(outputs.allocator().clone(), planned.mode,
                prepared.format, prepared.modifiers.clone(), &prepared.properties, false)).map_err(error)?;
            screen.adaptive_sync.prepared = Some(false);
            screen.adaptive_sync.reason = Some(format!("The combined output candidate refused adaptive sync; fixed refresh was retained: {failure}"));
        }
        screen.color.status.revision = revision;
        screen.color.status.pending = true;
        screen.adaptive_sync.pending = true;
        screen.texture = None;
    }
    screen.color.status.hdr10 = supported;
    screen.color.status.prepared = Some(prepared.output);
    screen.color.status.peak_reported = prepared.reported;
    screen.color.status.reason = reason;
    screen.color.power_reason = None;
    screen.color.prepared = Some(prepared);
    Ok(())
}

fn observed(device: &impl control::Device, connector: connector::Handle, plane: plane::Handle, pipeline: &Pipeline) -> io::Result<()> {
    let properties = device.get_properties(connector)?;
    for (_, name, expected) in &pipeline.properties {
        let mut found = None;
        for (handle, value) in &properties {
            if device.get_property(*handle)?.name().to_bytes() == name.as_bytes() { found = Some(*value); break; }
        }
        if found != Some(*expected) { return Err(io::Error::other(format!("The kernel-observed {name} differs from the prepared color request."))); }
    }
    let plane = device.get_plane(plane)?;
    let framebuffer = plane.framebuffer().ok_or_else(|| io::Error::other("The primary plane has no active framebuffer to confirm output color."))?;
    let format = device.get_planar_framebuffer(framebuffer).map_err(error)?.pixel_format();
    let opaque = smithay::backend::allocator::format::get_opaque(pipeline.format).unwrap_or(pipeline.format);
    if format != pipeline.format && format != opaque { return Err(io::Error::other("The active framebuffer format differs from the prepared color encoding.")); }
    Ok(())
}

pub(in super::super) fn observe(device: &impl control::Device, screen: &mut Screen, submitted: &Submitted) -> bool {
    if !screen.color.matches_submission(submitted) { return false; }
    let connector = screen.connector;
    let plane = screen.output.with_compositor(|compositor| compositor.surface().plane());
    screen.color.accept_observation(submitted, || observed(device, connector, plane, &submitted.pipeline))
}

impl Hardware {
    pub(in super::super) fn color_ready(&self) -> bool {
        self.screens.values().filter(|screen| screen.power.on).all(|screen| screen.color.status.prepared.is_none_or(|prepared|
            prepared.mode == ColorMode::Srgb || !screen.color.status.pending && screen.color.status.effective == Some(prepared)
                && screen.color.active.as_ref().is_some_and(|pipeline| pipeline.output == prepared)))
    }

    pub(in super::super) fn publish_color(&self, server: &mut Server) {
        for head in &self.catalog {
            let status = self.screens.values().find(|screen| screen.id == head.id).map(|screen| screen.color.status.clone())
                .unwrap_or_else(|| ColorStatus { reason: Some("Enable this output to discover its color capabilities.".into()), ..Default::default() });
            server.output_color(head.id, status);
        }
    }

    pub(super) fn test_color(&self, plan: &OutputPlan) -> io::Result<()> {
        for planned in &plan.screens {
            let Some(screen) = self.screens.get(&planned.crtc).filter(|screen| screen.connector == planned.connector) else {
                if planned.color == ColorMode::Hdr10 { return Err(io::Error::other("Enable this output with SDR before testing HDR10.")); }
                continue;
            };
            let pipeline = if planned.color == ColorMode::Hdr10 {
                screen.color.hdr.as_ref().filter(|_| screen.color.status.hdr10)
            } else { screen.color.prepared.as_ref().filter(|pipeline| pipeline.output.mode == ColorMode::Srgb) };
            if let Some(pipeline) = pipeline {
                screen.output.with_compositor(|compositor| compositor.test_color_format(self.outputs.allocator().clone(), planned.mode,
                    pipeline.format, pipeline.modifiers.clone(), &pipeline.properties, planned.adaptive_sync)).map_err(error)?;
            } else if planned.color == ColorMode::Hdr10 {
                return Err(io::Error::other(screen.color.status.reason.clone().unwrap_or_else(|| "HDR10 is unavailable on this output.".into())));
            } else if let Some((format, properties)) = &screen.color.baseline {
                screen.output.with_compositor(|compositor| compositor.test_color_format(self.outputs.allocator().clone(), planned.mode,
                    *format, modifiers(&self.renderer, *format), properties, planned.adaptive_sync)).map_err(error)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_output_metadata_uses_the_declared_target_volume_and_no_client_light_claim() {
        let bytes = metadata(OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 });
        let word = |offset| u16::from_ne_bytes(bytes[offset..offset + 2].try_into().unwrap());
        assert_eq!(&bytes[..6], &[0, 0, 0, 0, 2, 0]);
        assert_eq!([word(6), word(8), word(10), word(12), word(14), word(16)], [35400, 14600, 8500, 39850, 6550, 2300]);
        assert_eq!([word(18), word(20), word(22), word(24), word(26), word(28)], [15635, 16450, 1000, 50, 0, 0]);
        assert_eq!(&bytes[30..], &[0, 0]);
    }
}
