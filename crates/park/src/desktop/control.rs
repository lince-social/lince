use super::*;
use crate::server::control::Effect;
use lince_interface::host::{control::{Disposition, Error, Failure, Item}, launch::Capability};

impl Desktop {
    pub(super) fn control(&mut self) -> io::Result<()> {
        use std::sync::mpsc::TrySendError;
        for work in self.server.control_work() {
            if !self.focused {
                work.complete(Err(Failure::new(Error::Unavailable, "The Park seat or nested view is inactive.")));
                continue;
            }
            match self.worker.input(Input::Control(work)) {
                Ok(serial) => {
                    self.pending_output = Some((serial, None));
                    self.native_keyboard = false;
                }
                Err(TrySendError::Full(Input::Control(work))) => work.complete(Err(Failure::new(Error::Busy, "The desktop input queue is busy. Retry after refreshing its state."))),
                Err(TrySendError::Disconnected(Input::Control(work))) => work.complete(Err(Failure::new(Error::Cancelled, "The desktop worker is unavailable."))),
                Err(_) => return Err(io::Error::other("The management input returned an incompatible message")),
            }
        }
        for command in self.server.control_hosts() {
            let result = command.valid().and_then(|_| {
                if !self.focused || self.server.locked() { return Err(Failure::new(Error::Unavailable, "The Park seat is inactive or locked.")); }
                match command.effect {
                    Effect::Launch(action) => {
                        self.launch_command(action, false, Some(&command.grants)).map_err(|error| Failure::new(
                            if error.kind() == io::ErrorKind::PermissionDenied { Error::Denied } else { Error::Unavailable }, error.to_string(),
                        ))?;
                        Ok(Disposition::Dispatched)
                    }
                    Effect::Close(surface) => {
                        if !command.grants.contains(&Capability::DesktopControl) || !command.grants.contains(&Capability::WindowControl)
                            || !matches!(command.invocation.context.item, Some(Item::Application { surface: captured, .. }) if captured == surface.0)
                        { return Err(Failure::new(Error::Denied, "The close request does not belong to this connection's captured window.")); }
                        if !self.server.close(surface) { return Err(Failure::new(Error::StaleTarget, "The captured application window is no longer available.")); }
                        Ok(Disposition::Dispatched)
                    }
                    Effect::Magnification(request) => {
                        if !command.grants.contains(&Capability::DesktopControl) || !command.grants.contains(&Capability::OutputControl) {
                            return Err(Failure::new(Error::Denied, "Display magnification requires desktop and output control permissions."));
                        }
                        if request.context != command.invocation.context {
                            return Err(Failure::new(Error::StaleTarget, "The magnification request no longer belongs to the captured Park view."));
                        }
                        let id = request.id;
                        self.magnification_request(request).map_err(|error| Failure::new(Error::Unavailable, error.to_string()))?;
                        match self.magnification.snapshot.response.as_ref().filter(|response| response.request == id).map(|response| &response.outcome) {
                            Some(lince_interface::host::magnification::Outcome::Saved { .. }) => Ok(Disposition::Dispatched),
                            Some(lince_interface::host::magnification::Outcome::Failed(reason)) => Err(Failure::new(Error::Unavailable, reason.clone())),
                            _ => Err(Failure::new(Error::Cancelled, "The original magnification request was retired.")),
                        }
                    }
                }
            });
            self.server.complete_control(command.ticket, result);
        }
        let status = self.server.control_status();
        if self.control_status.as_ref() != Some(&status)
            && self.worker.input(Input::ControlStatus(status.clone())).is_ok()
        { self.control_status = Some(status); }
        Ok(())
    }
}
