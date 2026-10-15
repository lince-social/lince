use super::*;
use smithay::{
    backend::allocator::{Buffer, Format, Fourcc, Modifier, dmabuf::Dmabuf},
    delegate_dmabuf,
    wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
};

pub(super) struct Buffers {
    state: DmabufState,
    global: Option<DmabufGlobal>,
}

impl Buffers {
    pub(super) fn new() -> Self {
        Self {
            state: DmabufState::new(),
            global: None,
        }
    }
}

impl Server {
    pub fn enable_dmabuf(&mut self) {
        if self.state.dmabuf.global.is_none() {
            self.state.dmabuf.global = Some(self.state.dmabuf.state.create_global::<State>(
                &self.display.handle(),
                [Format {
                    code: Fourcc::Argb8888,
                    modifier: Modifier::Linear,
                }],
            ));
        }
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf.state
    }

    fn dmabuf_imported(&mut self, _: &DmabufGlobal, buffer: Dmabuf, notifier: ImportNotifier) {
        let size = buffer.size();
        if buffer.num_planes() != 1
            || buffer.format().code != Fourcc::Argb8888
            || buffer.format().modifier != Modifier::Linear
            || buffer.y_inverted()
            || validate_size([size.w, size.h]).is_err()
            || i64::from(size.w) * i64::from(size.h) * 4 > 256 * 1024 * 1024
        {
            notifier.failed();
        } else {
            let _ = notifier.successful::<Self>();
        }
    }
}

delegate_dmabuf!(State);
