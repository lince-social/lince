use super::*;
use std::sync::mpsc::TrySendError;

impl Desktop {
    #[cfg(all(test, feature = "server-tests", feature = "nested"))]
    pub(crate) fn file_hover_observation(&self) -> (Option<FileDragContext>, Option<OverviewHoverRequest>) {
        (self.file_drag_sent, self.hover_reply.map(|(request, _)| request))
    }

    pub(super) fn file_drag_context(&self) -> Option<FileDragContext> {
        if !self.focused || !self.server.dragging() || self.server.locked() || self.pending_output.is_some() {
            return None;
        }
        let token = self.server.native_file_drag()?;
        let frame = &self.current.as_ref()?.frame;
        let owner = frame.input.workspaces.owner;
        (owner != 0).then_some(FileDragContext {
            owner,
            output: frame.output,
            token,
        })
    }

    pub(super) fn cancel_pointer_drag(&mut self, time: u32) -> io::Result<()> {
        let buttons = self.server.cancel_pointer_drag(time);
        self.manipulation.swallow_releases(buttons);
        self.grabbed = None;
        self.reconcile_file_drag()?;
        self.redraw = true;
        Ok(())
    }

    pub(super) fn reconcile_file_drag(&mut self) -> io::Result<()> {
        let active = self.server.dragging();
        if active != self.drag_active_sent {
            match self.worker.input(Input::DragActive(active)) {
                Ok(_) => self.drag_active_sent = active,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas stopped while updating drag ownership")),
            }
        }
        let context = self.file_drag_context();
        if self.file_drag_sent != context {
            match self.worker.input(Input::FileDrag(context)) {
                Ok(_) => self.file_drag_sent = context,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err(io::Error::other(
                        "The Park canvas stopped during a file drag",
                    ));
                }
            }
        }
        if let Some((request, accepted)) = self.hover_reply {
            let accepted = accepted && self.file_drag_context() == Some(request.drag);
            match self
                .worker
                .input(Input::OverviewHover { request, accepted })
            {
                Ok(serial) => {
                    self.hover_reply = None;
                    if accepted {
                        self.pending_output = Some((serial, None));
                        self.native_keyboard = false;
                    }
                }
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err(io::Error::other(
                        "The Park canvas stopped before confirming file hover",
                    ));
                }
            }
        }
        Ok(())
    }
}
