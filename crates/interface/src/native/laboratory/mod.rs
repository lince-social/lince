// non-lean-yet
// Scope: mod catalogue, mod components, mod driver, mod interface.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 127.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (195, 215).
// - [review: §5 Panics] Verify panic isolation boundary and preserve the failure contract. Sites
//   62.

mod catalogue;
pub mod components;
#[cfg(feature = "test-driver")]
pub mod driver;
mod interface;
mod isolation;
mod park_report;
mod requirements;
pub mod showoff;
mod stress;
mod workspace;
pub(crate) use isolation::{LaboratoryOwner, OwnerAccess};
pub use isolation::{blocked, temporary_owner};
pub(crate) use isolation::{reserved as reserved_source, source as owner_source};

pub use catalogue::catalogue;
pub use interface::{run_interface_checks, run_interface_checks_with_timeout};
pub use requirements::RuleCoverage;
pub use stress::{Measurement, StressConfig, StressReport};
pub use workspace::{
    Laboratory, LaboratoryAction, LaboratoryPlugin, LaboratoryRoot, active, close, normal, scope,
    suspended, temporary,
};

use crate::testing::{Cancellation, InterfaceSelection};
use serde::Serialize;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
    thread,
    time::Instant,
};
pub(crate) use workspace::resources_visible;

#[derive(Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub source: CaseSource,
    pub run: fn(&tokio::runtime::Runtime),
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CaseSource {
    pub path: &'static str,
    pub registration_line: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CaseSource>,
    pub milliseconds: f64,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<RuleCoverage>,
}

impl Case {
    pub fn execute(&self, runtime: &tokio::runtime::Runtime) -> CaseResult {
        let start = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| (self.run)(runtime)));
        CaseResult {
            name: self.name.into(),
            source: Some(self.source),
            milliseconds: start.elapsed().as_secs_f64() * 1000.0,
            rules: Vec::new(),
            error: result.err().map(|error| {
                error
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        error
                            .downcast_ref::<&str>()
                            .map(|message| (*message).into())
                    })
                    .unwrap_or_else(|| "Behavior check panicked".into())
            }),
        }
    }
}

pub struct BehaviorRun {
    pub total: usize,
    pub results: Vec<CaseResult>,
    pub current: Option<String>,
    pub cancelled: bool,
    pub(super) selection: Option<InterfaceSelection>,
    cancel: Cancellation,
    incoming: std::sync::Mutex<mpsc::Receiver<Progress>>,
    task: Option<thread::JoinHandle<()>>,
}

enum Progress {
    Started(String),
    Finished(CaseResult),
    Done,
}

impl BehaviorRun {
    pub fn start(wake: Option<crate::native::wake::WakeSignal>) -> std::io::Result<Self> {
        Self::start_cases(catalogue(), wake, None)
    }

    pub fn start_interface(wake: Option<crate::native::wake::WakeSignal>) -> std::io::Result<Self> {
        Self::start_interface_selected(wake, InterfaceSelection::All)
    }

    pub fn start_interface_selected(
        wake: Option<crate::native::wake::WakeSignal>,
        selection: InterfaceSelection,
    ) -> std::io::Result<Self> {
        Self::start_cases(interface::fast_cases(), wake, Some(selection))
    }

    fn start_cases(
        cases: Vec<Case>,
        wake: Option<crate::native::wake::WakeSignal>,
        selection: Option<InterfaceSelection>,
    ) -> std::io::Result<Self> {
        let (selected, full_suite) = match selection {
            Some(selection) => (interface::select_cases(&cases, selection)?, Some(cases)),
            None => (cases, None),
        };
        let total = selected.len();
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let (sender, incoming) = mpsc::channel();
        let task = thread::Builder::new()
            .name("interface-behavior".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let execution = (|| -> std::io::Result<()> {
                    let mut rules = if let Some(cases) = full_suite {
                        requirements::for_cases(&cases)?
                    } else {
                        Default::default()
                    };
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .thread_stack_size(32 * 1024 * 1024)
                        .enable_all()
                        .build()?;
                    for case in selected {
                        if stopped.is_cancelled()
                            || sender.send(Progress::Started(case.name.into())).is_err()
                        {
                            break;
                        }
                        if let Some(wake) = &wake {
                            wake.ring();
                        }
                        let mut result = stopped.scope(|| case.execute(&runtime));
                        if stopped.is_cancelled() {
                            break;
                        }
                        result.rules = rules.remove(case.name).unwrap_or_default();
                        if sender.send(Progress::Finished(result)).is_err() {
                            break;
                        }
                        if let Some(wake) = &wake {
                            wake.ring();
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = execution {
                    let _ = sender.send(Progress::Finished(CaseResult {
                        name: "Behavior runner".into(),
                        source: None,
                        milliseconds: 0.0,
                        error: Some(error.to_string()),
                        rules: Vec::new(),
                    }));
                }
                let _ = sender.send(Progress::Done);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            })?;
        Ok(Self {
            total,
            results: Vec::new(),
            current: None,
            cancelled: false,
            selection,
            cancel,
            incoming: std::sync::Mutex::new(incoming),
            task: Some(task),
        })
    }

    fn drain(&mut self) -> bool {
        let mut done = false;
        while let Ok(progress) = self.incoming.get_mut().unwrap().try_recv() {
            match progress {
                Progress::Done => done = true,
                Progress::Started(name) => self.current = Some(name),
                Progress::Finished(result) => {
                    self.results.push(result);
                    self.current = None;
                }
            }
        }
        done
    }

    pub fn poll(&mut self) {
        let done = self.drain();
        if self
            .task
            .as_ref()
            .is_some_and(|task| done || task.is_finished())
        {
            if self.task.take().unwrap().join().is_err() {
                self.results.push(CaseResult {
                    name: "Behavior runner".into(),
                    source: None,
                    milliseconds: 0.0,
                    error: Some("Runner stopped unexpectedly".into()),
                    rules: Vec::new(),
                });
            }
            self.drain();
            self.current = None;
        }
    }

    pub fn finished(&self) -> bool {
        self.task.is_none()
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.cancel.cancel();
    }
}

impl Drop for BehaviorRun {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[macro_export]
macro_rules! laboratory_cases {
    (@add $cases:ident;) => {};
    (@add $cases:ident; $(#[$meta:meta])* async $name:ident, $($rest:tt)*) => {
        $crate::native::laboratory_cases!(@add $cases; $(#[$meta])* async $name timeout 30, $($rest)*);
    };
    (@add $cases:ident; $(#[$meta:meta])* async $name:ident timeout $seconds:literal, $($rest:tt)*) => {
        $(#[$meta])*
        $cases.push($crate::native::laboratory::Case {
            name: concat!(module_path!(), "::", stringify!($name)),
            source: $crate::native::laboratory::CaseSource { path: file!(), registration_line: line!() },
            run: |_runtime| {
                #[cfg(test)]
                $name();
                #[cfg(not(test))]
                _runtime.block_on(async {
                    let cancellation = $crate::testing::Cancellation::current();
                    tokio::select! {
                        biased;
                        () = cancellation.cancelled() => (),
                        result = tokio::time::timeout(std::time::Duration::from_secs($seconds), $name()) => {
                            result.expect("behavior check timed out");
                        }
                    }
                });
            },
        });
        $crate::native::laboratory_cases!(@add $cases; $($rest)*);
    };
    (@add $cases:ident; $(#[$meta:meta])* $name:ident, $($rest:tt)*) => {
        $(#[$meta])*
        $cases.push($crate::native::laboratory::Case {
            name: concat!(module_path!(), "::", stringify!($name)),
            source: $crate::native::laboratory::CaseSource { path: file!(), registration_line: line!() },
            run: |_| $name(),
        });
        $crate::native::laboratory_cases!(@add $cases; $($rest)*);
    };
    ($($cases:tt)*) => {
        pub(crate) fn laboratory_cases(cases: &mut Vec<$crate::native::laboratory::Case>) {
            $crate::native::laboratory_cases!(@add cases; $($cases)*);
        }
    };
}

pub mod fiote;
mod headless;
pub use headless::{HeadlessReport, run_headless};

mod tests;

pub fn isolate(world: &mut bevy::prelude::World) {
    world.insert_resource(crate::native::notifications::Notifications::new(
        cell::Diagnostics::default(),
    ));
}

pub(crate) mod resource_tests;
pub mod resources;
pub use resources::{
    GraphicsDevice, ResourceSnapshot, SandResources, StartupFailure, StartupIssue,
};
