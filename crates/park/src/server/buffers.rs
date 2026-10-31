use super::*;
use smithay::{
    backend::allocator::{Buffer, Format, Fourcc, Modifier, dmabuf::Dmabuf},
    delegate_dmabuf,
    wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
};

pub(super) struct Buffers {
    state: DmabufState,
    global: Option<DmabufGlobal>,
    formats: Vec<Format>,
}

impl Buffers {
    pub(super) fn new() -> Self {
        Self {
            state: DmabufState::new(),
            global: None,
            formats: Vec::new(),
        }
    }
}

impl Server {
    pub fn enable_dmabuf_formats(&mut self, formats: impl IntoIterator<Item = Format>, device: impl FnOnce() -> Option<rustix::fs::Dev>) -> io::Result<()> {
        if self.state.dmabuf.global.is_none() {
            let mut accepted = Vec::new();
            for format in formats {
                if format.modifier == Modifier::Linear
                    && matches!(format.code, Fourcc::Argb8888 | Fourcc::Abgr8888 | Fourcc::Abgr2101010 | Fourcc::Abgr16161616f)
                    && !accepted.contains(&format) { accepted.push(format); }
            }
            if accepted.is_empty() { return Ok(()); }
            self.state.dmabuf.global = Some(if let Some(device) = device() {
                let feedback = DmabufFeedbackBuilder::new(device, accepted.iter().copied()).build().map_err(io::Error::other)?;
                self.state.dmabuf.state.create_global_with_default_feedback::<State>(&self.display.handle(), &feedback)
            } else {
                self.state.dmabuf.state.create_global::<State>(&self.display.handle(), accepted.iter().copied())
            });
            self.state.dmabuf.formats = accepted;
        }
        Ok(())
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.state
    }

    fn dmabuf_imported(&mut self, _: &DmabufGlobal, buffer: Dmabuf, notifier: ImportNotifier) {
        let size = buffer.size();
        let bytes_per_pixel = if buffer.format().code == Fourcc::Abgr16161616f { 8 } else { 4 };
        if buffer.num_planes() != 1
            || !self.dmabuf.formats.contains(&buffer.format())
            || buffer.format().modifier != Modifier::Linear
            || buffer.y_inverted()
            || validate_size([size.w, size.h]).is_err()
            || i64::from(size.w) * i64::from(size.h) * bytes_per_pixel > 256 * 1024 * 1024
        {
            notifier.failed();
        } else {
            let _ = notifier.successful::<Self>();
        }
    }
}

delegate_dmabuf!(State);
