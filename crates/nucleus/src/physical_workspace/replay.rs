use serde::{Deserialize, Serialize};

use super::{Commit, Configuration, Evidence, Interaction, Request, Runtime, Target};
use crate::json::encoded_len;

const MAX_STEPS: usize = 512;
const MAX_TRACE_BYTES: usize = 1024 * 1024;

/// Operation recorded in an ordered Physical Workspace replay.
#[expect(clippy::exhaustive_enums, reason = "the replay vocabulary is public")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Event {
    /// Connects a device under a session identity.
    Connect {
        /// Device to connect.
        device: String,
        /// Session identity for the connection.
        session: String,
    },
    /// Disconnects a device.
    Disconnect {
        /// Device to disconnect.
        device: String,
    },
    /// Sets camera capture permission.
    Capture {
        /// Camera whose capture permission changes.
        device: String,
        /// Whether capture is permitted.
        enabled: bool,
    },
    /// Submits an observation to the workspace.
    Observe {
        /// Evidence from the camera.
        evidence: Evidence,
    },
    /// Applies one pointer interaction.
    Interact {
        /// Pointer phase and evidence.
        interaction: Interaction,
    },
    /// Queues a request built from workspace observations.
    Enqueue {
        /// Request to queue.
        request: Request,
    },
    /// Starts any requests eligible for dispatch at this step.
    Dispatch,
    /// Cancels one request.
    Cancel {
        /// Request identifier.
        request: String,
    },
    /// Submits a result for a dispatched request.
    Finish {
        /// Request identifier.
        request: String,
        /// Selected target for the result.
        target: Target,
        /// User completing the request.
        user: String,
        /// Result text.
        result: String,
    },
    /// Sets whether projection is blanked.
    Blank {
        /// Whether projection is blanked.
        enabled: bool,
    },
}

/// One replay event and its logical timestamp.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Workspace clock time in milliseconds.
    pub now_ms: u64,
    /// Operation to attempt at this time.
    pub event: Event,
}

/// Initial configuration and ordered steps for a workspace replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    /// Workspace identifier for scoped operations.
    pub workspace: u64,
    /// Configuration used to initialize the runtime.
    pub configuration: Configuration,
    /// Events in replay order.
    pub steps: Vec<Step>,
}

/// Outcomes collected while applying a replay trace.
#[derive(Debug, Serialize)]
pub struct Report {
    /// Number of events accepted by the runtime.
    pub accepted: usize,
    /// Failed events by zero-based trace index, in replay order.
    pub errors: Vec<(usize, String)>,
    /// Commits produced by successful interactions.
    pub commits: Vec<Commit>,
    /// Request identifiers in dispatch order.
    pub dispatched: Vec<String>,
    /// Number of observations retained after the final step.
    pub remaining_observations: usize,
    /// Number of pointers retained after the final step.
    pub remaining_pointers: usize,
}

impl Trace {
    /// Applies the trace in order, recording failed events and continuing the replay.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for an invalid trace envelope or runtime configuration.
    pub fn run(&self) -> Result<(Runtime, Report), String> {
        if self.workspace == 0
            || self.steps.len() > MAX_STEPS
            || encoded_len(self).map_err(|error| error.to_string())? > MAX_TRACE_BYTES
        {
            return Err("Use a replay of at most 512 events and 1 MiB.".into());
        }
        let workspace = self.workspace;
        let mut runtime = Runtime::new(self.configuration.clone())?;
        let mut accepted = 0;
        let mut errors = Vec::new();
        let mut commits = Vec::new();
        let mut dispatched = Vec::new();
        for (index, Step { now_ms, event }) in self.steps.iter().enumerate() {
            let now_ms = *now_ms;
            let result = runtime.advance(now_ms).and_then(|()| match event {
                Event::Connect { device, session } => runtime.connect(device, session),
                Event::Disconnect { device } => {
                    runtime.disconnect(device);
                    Ok(())
                }
                Event::Capture { device, enabled } => runtime.capture(device, *enabled),
                Event::Observe { evidence } => runtime.observe(workspace, evidence.clone(), now_ms),
                Event::Interact { interaction } => {
                    commits.extend(runtime.interact(workspace, interaction.clone(), now_ms)?);
                    Ok(())
                }
                Event::Enqueue { request } => runtime.enqueue(request.clone(), workspace, now_ms),
                Event::Dispatch => {
                    dispatched.extend(
                        runtime
                            .dispatch(workspace, now_ms)?
                            .into_iter()
                            .map(|r| r.id),
                    );
                    Ok(())
                }
                Event::Cancel { request } => runtime.cancel(request),
                Event::Finish {
                    request,
                    target,
                    user,
                    result,
                } => runtime.finish(request, target, user, result.clone(), now_ms),
                Event::Blank { enabled } => {
                    runtime.blank(*enabled);
                    Ok(())
                }
            });
            match result {
                Ok(()) => accepted += 1,
                Err(error) => errors.push((index, error)),
            }
        }
        let report = Report {
            accepted,
            errors,
            commits,
            dispatched,
            remaining_observations: runtime.observations.len(),
            remaining_pointers: runtime.pointers.len(),
        };
        Ok((runtime, report))
    }
}
