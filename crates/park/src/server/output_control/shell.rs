use super::*;
use lince_interface::host::display::{
    DisplayAction, DisplayConfiguration, DisplayIdentification, DisplayOutcome, DisplayPreview,
    DisplayRequest, DisplayResponse, DisplaySnapshot,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Shell {
    work: Option<Work>,
    preview: Option<Preview>,
    identification: Option<DisplayIdentification>,
    response: Option<DisplayResponse>,
    last_request: u64,
    kept_token: Option<u64>,
    virtual_bounds: Option<[u32; 2]>,
}

struct Work {
    token: u64,
    request: u64,
    heads: DisplayConfiguration,
    phase: Phase,
}

enum Phase {
    Test,
    TestPreview(Vec<OutputHead>),
    ApplyPreview(Vec<OutputHead>),
    Revert {
        before: Vec<OutputHead>,
        attempt: u8,
        failure: Option<String>,
    },
}

struct Preview {
    state: DisplayPreview,
    request: u64,
    before: Vec<OutputHead>,
    applied_revision: u32,
}

impl Shell {
    pub(super) fn owns(&self, token: u64) -> bool {
        self.work.as_ref().is_some_and(|work| work.token == token)
    }

    pub(super) fn recovering(&self, token: u64) -> bool {
        self.work
            .as_ref()
            .is_some_and(|work| work.token == token && matches!(work.phase, Phase::Revert { .. }))
    }

    pub(super) fn busy(&self) -> bool {
        self.work.is_some() || self.preview.is_some()
    }
}

impl Server {
    pub fn display_preview_token(&self) -> Option<u64> {
        self.state
            .output_control
            .shell
            .preview
            .as_ref()
            .map(|preview| preview.state.token)
    }

    pub fn kept_display_token(&self) -> Option<u64> {
        self.state.output_control.shell.kept_token
    }

    pub fn output_configuration_preview(&self, request: &OutputRequest) -> bool {
        self.state
            .output_control
            .shell
            .work
            .as_ref()
            .is_some_and(|work| {
                work.token == request.token && matches!(work.phase, Phase::ApplyPreview(_))
            })
    }

    pub fn display_snapshot(&self) -> DisplaySnapshot {
        let control = &self.state.output_control;
        DisplaySnapshot {
            revision: u64::from(control.serial),
            heads: control.catalog.clone(),
            adaptive_sync: control.adaptive_sync.clone(),
            color: control.color.clone(),
            outputs: self
                .output_viewports()
                .into_iter()
                .map(|(_, viewport)| viewport)
                .collect(),
            virtual_bounds: control.shell.virtual_bounds,
            preview: control
                .shell
                .preview
                .as_ref()
                .map(|preview| preview.state.clone()),
            identification: control.shell.identification.clone(),
            response: control.shell.response.clone(),
            last_request: control.shell.last_request,
        }
    }

    pub fn display_bounds(&mut self, bounds: Option<[u32; 2]>) {
        if self.state.output_control.shell.virtual_bounds != bounds {
            self.state.output_control.shell.virtual_bounds = bounds;
            self.state.presentation_changed();
        }
    }

    pub fn display_timeout(&self) -> Option<Duration> {
        let shell = &self.state.output_control.shell;
        shell
            .preview
            .as_ref()
            .filter(|preview| !preview.state.reverting)
            .map(|preview| preview.state.deadline)
            .into_iter()
            .chain(
                shell
                    .identification
                    .as_ref()
                    .map(|identification| identification.deadline),
            )
            .min()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    fn display_response(&mut self, request: u64, outcome: DisplayOutcome) {
        self.state.output_control.shell.response = Some(DisplayResponse { request, outcome });
        self.state.presentation_changed();
    }

    pub fn display_request(
        &mut self,
        request: DisplayRequest,
        persist: impl FnOnce(&[OutputHead]) -> io::Result<()>,
    ) {
        self.advance_display(Instant::now());
        let shell = &mut self.state.output_control.shell;
        if request.id == 0 || request.id <= shell.last_request {
            self.display_response(
                request.id,
                DisplayOutcome::Failed(
                    "This display request has already expired; refresh the monitor settings."
                        .into(),
                ),
            );
            return;
        }
        shell.last_request = request.id;
        if let DisplayAction::Cancel { request: pending } = &request.action {
            self.cancel_display(request.id, *pending);
            return;
        }
        if self.locked() {
            self.display_response(
                request.id,
                DisplayOutcome::Failed(
                    "Display settings are unavailable while the session is locked.".into(),
                ),
            );
            return;
        }
        let preview_requested = matches!(&request.action, DisplayAction::Preview { .. });
        match request.action {
            DisplayAction::Test { revision, heads }
            | DisplayAction::Preview { revision, heads } => {
                let control = &self.state.output_control;
                if control.shell.busy() {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(
                            "Keep or revert the current display changes first.".into(),
                        ),
                    );
                } else if revision != u64::from(control.serial) {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(
                            "The connected monitors changed; reload the draft before applying it."
                                .into(),
                        ),
                    );
                } else if !valid_configuration(&control.catalog, &heads) {
                    self.display_response(request.id, DisplayOutcome::Failed("Choose supported modes and scales, valid positions, and keep at least one monitor enabled.".into()));
                } else {
                    let phase = if preview_requested {
                        Phase::TestPreview(control.catalog.clone())
                    } else {
                        Phase::Test
                    };
                    if self.queue_display_work(request.id, heads, phase) {
                        self.display_response(request.id, DisplayOutcome::Pending);
                    }
                }
            }
            DisplayAction::Keep { token } => {
                let valid = self
                    .state
                    .output_control
                    .shell
                    .preview
                    .as_ref()
                    .is_some_and(|preview| {
                        preview.state.token == token
                            && !preview.state.reverting
                            && preview.state.deadline > Instant::now()
                    });
                if !valid {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(
                            "These display changes are no longer awaiting confirmation.".into(),
                        ),
                    );
                } else if !lince_interface::host::display::color_confirmation_ready(&self.state.output_control.catalog, &self.state.output_control.color) {
                    self.display_response(request.id, DisplayOutcome::Failed("Wait for the HDR output to be confirmed before keeping these settings, or revert.".into()));
                } else if let Err(error) = persist(&self.state.output_control.catalog) {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(format!("Could not save display settings: {error}")),
                    );
                } else {
                    self.state.output_control.shell.kept_token = Some(token);
                    self.state.output_control.shell.preview = None;
                    self.display_response(request.id, DisplayOutcome::Kept);
                }
            }
            DisplayAction::Revert { token } => {
                if let Some(preview) = self
                    .state
                    .output_control
                    .shell
                    .preview
                    .as_ref()
                    .filter(|preview| preview.state.token == token && !preview.state.reverting)
                {
                    let before = preview.before.clone();
                    self.recover_display(request.id, before, 0, None);
                } else {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(
                            "These display changes are no longer awaiting confirmation.".into(),
                        ),
                    );
                }
            }
            DisplayAction::Cancel { .. } => unreachable!(),
            DisplayAction::Identify { output } => {
                if self
                    .state
                    .output_control
                    .catalog
                    .iter()
                    .any(|head| head.id == output && head.current.is_some())
                {
                    self.state.output_control.shell.identification = Some(DisplayIdentification {
                        output,
                        deadline: Instant::now() + Duration::from_secs(3),
                    });
                    self.display_response(request.id, DisplayOutcome::Identified);
                } else {
                    self.display_response(
                        request.id,
                        DisplayOutcome::Failed(
                            "Enable this connected monitor before identifying it.".into(),
                        ),
                    );
                }
            }
        }
    }

    fn cancel_display(&mut self, request: u64, pending: u64) {
        let shell = &mut self.state.output_control.shell;
        if shell.work.as_ref().is_some_and(|work| {
            work.request == pending && matches!(work.phase, Phase::Revert { .. })
        }) {
            self.display_response(request, DisplayOutcome::Pending);
        } else if let Some(work) = shell.work.take_if(|work| work.request == pending) {
            self.state
                .output_control
                .requests
                .retain(|queued| queued.token != work.token);
            match work.phase {
                Phase::ApplyPreview(before) => self.recover_display(request, before, 0, None),
                Phase::Test | Phase::TestPreview(_) => {
                    self.display_response(request, DisplayOutcome::Reverted)
                }
                Phase::Revert { .. } => unreachable!(),
            }
        } else if let Some(preview) = shell
            .preview
            .as_ref()
            .filter(|preview| preview.request == pending && !preview.state.reverting)
        {
            let before = preview.before.clone();
            self.recover_display(request, before, 0, None);
        } else {
            self.display_response(
                request,
                DisplayOutcome::Failed("This display request is no longer active.".into()),
            );
        }
    }

    fn queue_display_work(
        &mut self,
        request: u64,
        heads: DisplayConfiguration,
        phase: Phase,
    ) -> bool {
        let control = &mut self.state.output_control;
        let recovering = matches!(&phase, Phase::Revert { .. });
        let Some(token) = control
            .next_token
            .checked_add(1)
            .filter(|_| recovering || control.requests.len() < 8)
        else {
            self.display_response(
                request,
                DisplayOutcome::Failed("The display backend is busy; try again.".into()),
            );
            return false;
        };
        control.next_token = token;
        let test = matches!(phase, Phase::Test | Phase::TestPreview(_));
        let pending = OutputRequest {
            token,
            serial: control.serial,
            test,
            heads: heads.clone(),
        };
        if recovering {
            control.requests.push_front(pending);
        } else {
            control.requests.push_back(pending);
        }
        control.shell.work = Some(Work {
            token,
            request,
            heads,
            phase,
        });
        self.state.presentation_changed();
        true
    }

    pub fn output_configuration_recovery(&self, request: &OutputRequest) -> bool {
        self.state.output_control.shell.recovering(request.token)
    }

    pub fn advance_display(&mut self, now: Instant) {
        let control = &mut self.state.output_control;
        if control
            .shell
            .identification
            .as_ref()
            .is_some_and(|identification| {
                identification.deadline <= now
                    || !control
                        .catalog
                        .iter()
                        .any(|head| head.id == identification.output && head.current.is_some())
            })
        {
            control.shell.identification = None;
            self.state.presentation_changed();
        }
        let control = &self.state.output_control;
        if let Some(preview) = control.shell.preview.as_ref().filter(|preview| {
            !preview.state.reverting
                && (preview.state.deadline <= now
                    || preview.applied_revision != control.serial
                    || self.locked())
        }) {
            let before = preview.before.clone();
            let request = preview.request;
            self.recover_display(request, before, 0, None);
        }
    }

    pub fn recover_display_preview(&mut self, reason: String) -> bool {
        if self.state.output_control.shell.work.as_ref().is_some_and(|work| matches!(work.phase, Phase::Revert { .. })) { return true; }
        let Some(preview) = self.state.output_control.shell.preview.as_ref().filter(|preview| !preview.state.reverting) else { return false; };
        let request = preview.request;
        let before = preview.before.clone();
        self.recover_display(request, before, 0, Some(format!("The display preview failed: {reason}. Previous settings were restored.")));
        self.state.output_control.shell.work.as_ref().is_some_and(|work| matches!(work.phase, Phase::Revert { .. }))
    }

    fn recover_display(
        &mut self,
        request: u64,
        before: Vec<OutputHead>,
        attempt: u8,
        failure: Option<String>,
    ) {
        if let Some(preview) = self.state.output_control.shell.preview.as_mut() {
            preview.state.reverting = true;
        }
        let catalog = &self.state.output_control.catalog;
        let mut heads: DisplayConfiguration = catalog
            .iter()
            .map(|head| {
                let previous = before.iter().find(|previous| previous.name == head.name);
                let settings = match previous {
                    Some(previous) if previous.current.is_none() => None,
                    Some(previous) => previous
                        .settings()
                        .filter(|settings| head.modes.contains(&settings.mode))
                        .or_else(|| fallback_settings(head)),
                    None => head.settings(),
                };
                (head.id, settings)
            })
            .collect();
        if attempt > 0 || !valid_configuration(catalog, &heads) {
            heads = catalog.iter().map(|head| (head.id, None)).collect();
            if let Some(head) = catalog
                .iter()
                .find(|head| head.current.is_some())
                .or_else(|| catalog.first())
                && let Some(mut settings) = fallback_settings(head)
            {
                settings.position = [0, 0];
                heads.insert(head.id, Some(settings));
            }
        }
        if !valid_configuration(catalog, &heads)
            || !self.queue_display_work(
                request,
                heads,
                Phase::Revert {
                    before,
                    attempt,
                    failure,
                },
            )
        {
            self.state.output_control.shell.preview = None;
            self.display_response(
                request,
                DisplayOutcome::Failed(
                    "No safe connected display configuration is currently available.".into(),
                ),
            );
        } else {
            self.display_response(request, DisplayOutcome::Pending);
        }
    }

    pub(super) fn finish_display_work(&mut self, token: u64, accepted: bool) -> bool {
        let Some(work) = self
            .state
            .output_control
            .shell
            .work
            .take_if(|work| work.token == token)
        else {
            return false;
        };
        match work.phase {
            Phase::Test => {
                self.display_response(work.request, if accepted { DisplayOutcome::Tested } else {
                    DisplayOutcome::Failed("The display backend rejected this configuration; no changes were applied.".into())
                });
            }
            Phase::TestPreview(before) => {
                if accepted {
                    self.queue_display_work(work.request, work.heads, Phase::ApplyPreview(before));
                } else {
                    self.display_response(
                        work.request,
                        DisplayOutcome::Failed(
                            "The display test failed; no changes were applied.".into(),
                        ),
                    );
                }
            }
            Phase::ApplyPreview(before) => {
                if accepted {
                    let control = &mut self.state.output_control;
                    control.shell.preview = Some(Preview {
                        state: DisplayPreview {
                            token,
                            deadline: Instant::now() + Duration::from_secs(15),
                            reverting: false,
                        },
                        request: work.request,
                        before,
                        applied_revision: control.serial,
                    });
                    self.display_response(work.request, DisplayOutcome::Previewed);
                } else {
                    self.recover_display(
                        work.request,
                        before,
                        0,
                        Some("The display apply failed; previous settings were restored.".into()),
                    );
                }
            }
            Phase::Revert {
                before,
                attempt,
                failure,
            } => {
                if accepted {
                    self.state.output_control.shell.preview = None;
                    self.display_response(
                        work.request,
                        failure
                            .map(DisplayOutcome::Failed)
                            .unwrap_or(DisplayOutcome::Reverted),
                    );
                } else if attempt < 2 {
                    self.recover_display(work.request, before, attempt + 1, failure);
                } else {
                    self.state.output_control.shell.preview = None;
                    self.display_response(work.request, DisplayOutcome::Failed("The display backend could not restore a safe layout; reconnect an available monitor.".into()));
                }
            }
        }
        true
    }
}

fn fallback_settings(head: &OutputHead) -> Option<OutputSettings> {
    let mode = head
        .current
        .and_then(|index| head.modes.get(index))
        .or_else(|| head.modes.iter().find(|mode| mode.preferred))
        .or_else(|| head.modes.first())?;
    Some(OutputSettings { color: head.color, adaptive_sync: head.adaptive_sync, rotation: head.rotation,
        mode: mode.clone(),
        position: head.position,
        scale: head.scale,
    })
}
