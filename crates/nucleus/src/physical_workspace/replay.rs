// non-lean-yet
// Scope: enum Event, struct Step, struct Trace, struct Report.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 74, 94, 98, 105,
//   122.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 5.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 67.
// - [Clippy: clippy::wildcard_imports] Use explicit imports while preserving framework and test
//   conventions. Sites 1.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   7, 8, 10, 11, 13, 14, 15, 17, 18, 20, 21, 23, 24, 26, 27, 28, 30, 31, 32, 33, 34, 36, 37, 43,
//   44, 45, 50, 51, 52, 53, 57, 58, 59, 60, 61, 62, 63, 67.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites run: String
//   (67).

use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Event {
    Connect {
        device: String,
        session: String,
    },
    Disconnect {
        device: String,
    },
    Capture {
        device: String,
        enabled: bool,
    },
    Observe {
        evidence: Evidence,
    },
    Interact {
        interaction: Interaction,
    },
    Enqueue {
        request: Request,
    },
    Dispatch,
    Cancel {
        request: String,
    },
    Finish {
        request: String,
        target: Target,
        user: String,
        result: String,
    },
    Blank {
        enabled: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub now_ms: u64,
    pub event: Event,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub workspace: u64,
    pub configuration: Configuration,
    pub steps: Vec<Step>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub accepted: usize,
    pub errors: Vec<(usize, String)>,
    pub commits: Vec<Commit>,
    pub dispatched: Vec<String>,
    pub remaining_observations: usize,
    pub remaining_pointers: usize,
}

impl Trace {
    pub fn run(&self) -> Result<(Runtime, Report), String> {
        if self.workspace == 0
            || self.steps.len() > 512
            || crate::json::encoded_len(self).map_err(|e| e.to_string())? > 1_048_576
        {
            return Err("Use a replay of at most 512 events and 1 MiB.".into());
        }
        let mut runtime = Runtime::new(self.configuration.clone())?;
        let mut report = Report {
            accepted: 0,
            errors: Vec::new(),
            commits: Vec::new(),
            dispatched: Vec::new(),
            remaining_observations: 0,
            remaining_pointers: 0,
        };
        for (index, step) in self.steps.iter().enumerate() {
            let result = (|| {
                runtime.advance(step.now_ms)?;
                match &step.event {
                    Event::Connect { device, session } => runtime.connect(device, session),
                    Event::Disconnect { device } => {
                        runtime.disconnect(device);
                        Ok(())
                    }
                    Event::Capture { device, enabled } => runtime.capture(device, *enabled),
                    Event::Observe { evidence } => {
                        runtime.observe(self.workspace, evidence.clone(), step.now_ms)
                    }
                    Event::Interact { interaction } => {
                        if let Some(commit) =
                            runtime.interact(self.workspace, interaction.clone(), step.now_ms)?
                        {
                            report.commits.push(commit);
                        }
                        Ok(())
                    }
                    Event::Enqueue { request } => {
                        runtime.enqueue(request.clone(), self.workspace, step.now_ms)
                    }
                    Event::Dispatch => {
                        report.dispatched.extend(
                            runtime
                                .dispatch(self.workspace, step.now_ms)?
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
                    } => runtime.finish(request, target, user, result.clone(), step.now_ms),
                    Event::Blank { enabled } => {
                        runtime.blank(*enabled);
                        Ok(())
                    }
                }
            })();
            match result {
                Ok(()) => report.accepted += 1,
                Err(error) => report.errors.push((index, error)),
            }
        }
        report.remaining_observations = runtime.observations.len();
        report.remaining_pointers = runtime.pointers.len();
        Ok((runtime, report))
    }
}
