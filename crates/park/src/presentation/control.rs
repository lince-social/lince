use super::*;
use crate::server::{ControlWork, control::Effect};
use lince_interface::host::{DesktopAction, HostRequest, control::{Disposition, Error, Failure}};

enum Execution { Complete(Disposition), Host(Effect) }

fn failed(error: ProbeFailure) -> Failure { Failure::new(Error::Unavailable, error.to_string()) }

impl Presentation {
    pub(crate) fn control(&mut self, work: ControlWork) {
        match self.execute_control(&work) {
            Ok(Execution::Complete(outcome)) => work.complete(Ok(outcome)),
            Ok(Execution::Host(effect)) => work.host(effect),
            Err(error) => work.complete(Err(error)),
        }
    }

    fn execute_control(&mut self, work: &ControlWork) -> Result<Execution, Failure> {
        use native::layout::desktop::control::{self, Prepared};
        work.valid()?;
        if self.suspended() { return Err(Failure::new(Error::Unavailable, "The Park view is inactive.")); }
        let root = self.canvas_root().map_err(failed)?;
        let prepared = control::prepare(self.app.world(), root, work.invocation(), work.grants())?;
        work.valid()?;
        let action = match prepared {
            Prepared::Workspace { owner, id } => {
                self.try_activate_workspace(owner, id).map_err(|error| Failure::new(Error::Unavailable, error.to_string()))?;
                return Ok(Execution::Complete(Disposition::Applied));
            }
            Prepared::Action(action) => action,
        };
        if let Some(result) = native::layout::desktop::apply_compact_action(self.app.world_mut(), root, action) {
            result.map_err(|reason| Failure::new(Error::Unavailable, reason))?;
            let outcome = if matches!(action, DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers) { Disposition::Dispatched } else { Disposition::Applied };
            return Ok(Execution::Complete(outcome));
        }
        if let Some(result) = native::layout::desktop::apply_scratch_action(self.app.world_mut(), root, action) {
            let dispatched = result.map_err(|reason| Failure::new(Error::Unavailable, reason))?;
            return Ok(Execution::Complete(if dispatched { Disposition::Dispatched } else { Disposition::Applied }));
        }
        if matches!(action, DesktopAction::OpenTerminal | DesktopAction::OpenLauncher | DesktopAction::External(_) | DesktopAction::Lock
            | DesktopAction::Close | DesktopAction::InspectWindowRecord | DesktopAction::ReapplyOpeningRules | DesktopAction::Magnify(_))
        {
            let requests = control::capture_requests(self.app.world_mut(), root, action)?;
            let Some(request) = requests.into_iter().next() else {
                if matches!(action, DesktopAction::Close | DesktopAction::InspectWindowRecord)
                    && matches!(work.invocation().context.item, Some(lince_interface::host::control::Item::Native { .. }))
                { return Ok(Execution::Complete(Disposition::Dispatched)); }
                return Err(Failure::new(Error::Unavailable, "The command did not produce its required host transaction."));
            };
            work.valid()?;
            match request {
                HostRequest::Action(action @ (DesktopAction::OpenTerminal | DesktopAction::OpenLauncher | DesktopAction::External(_) | DesktopAction::Lock)) => Ok(Execution::Host(Effect::Launch(action))),
                HostRequest::Close(surface) => Ok(Execution::Host(Effect::Close(surface))),
                HostRequest::Magnification(request) => Ok(Execution::Host(Effect::Magnification(request))),
                HostRequest::Inspect(target) => {
                    self.try_inspect_window(target).map_err(failed)?;
                    Ok(Execution::Complete(Disposition::Dispatched))
                }
                HostRequest::ReapplyOpening(target) => {
                    self.try_reapply_opening(target).map_err(failed)?;
                    Ok(Execution::Complete(Disposition::Applied))
                }
                _ => Err(Failure::new(Error::Unavailable, "The command returned an incompatible host transaction.")),
            }
        } else {
            if !self.try_output_action(action).map_err(|error| Failure::new(Error::Unavailable, error.to_string()))? {
                self.desktop_action(action).map_err(failed)?;
            }
            Ok(Execution::Complete(if matches!(action, DesktopAction::OpenOperation | DesktopAction::ToggleOverview | DesktopAction::ToggleEdit | DesktopAction::Logout) { Disposition::Dispatched } else { Disposition::Applied }))
        }
    }
}
