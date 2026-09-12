mod catalogue;
mod stress;
mod workspace;

pub use catalogue::catalogue;
pub use stress::{Measurement, StressConfig, StressReport};
pub use workspace::{
    Laboratory, LaboratoryAction, LaboratoryPlugin, LaboratoryRoot, active, close, normal,
    suspended,
};

use serde::Serialize;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

pub struct Case {
    pub name: &'static str,
    pub run: fn(&tokio::runtime::Runtime),
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseResult {
    pub name: String,
    pub milliseconds: f64,
    pub error: Option<String>,
}

impl Case {
    pub fn execute(&self, runtime: &tokio::runtime::Runtime) -> CaseResult {
        let start = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| (self.run)(runtime)));
        CaseResult {
            name: self.name.into(),
            milliseconds: start.elapsed().as_secs_f64() * 1000.0,
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
    cancel: Arc<AtomicBool>,
    incoming: std::sync::Mutex<mpsc::Receiver<Progress>>,
    task: Option<thread::JoinHandle<()>>,
}

enum Progress {
    Started(String),
    Finished(CaseResult),
    Done,
}

impl BehaviorRun {
    pub fn start(wake: Option<crate::wake::WakeSignal>) -> std::io::Result<Self> {
        let cases = catalogue();
        let total = cases.len();
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (sender, incoming) = mpsc::channel();
        let task = thread::Builder::new()
            .name("interface-behavior".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_stack_size(32 * 1024 * 1024)
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => {
                        for case in cases {
                            if stopped.load(Ordering::Acquire) {
                                break;
                            }
                            if sender.send(Progress::Started(case.name.into())).is_err() {
                                break;
                            }
                            if let Some(wake) = &wake {
                                wake.ring();
                            }
                            let result = case.execute(&runtime);
                            if sender.send(Progress::Finished(result)).is_err() {
                                break;
                            }
                            if let Some(wake) = &wake {
                                wake.ring();
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Progress::Finished(CaseResult {
                            name: "Behavior runner".into(),
                            milliseconds: 0.0,
                            error: Some(error.to_string()),
                        }));
                    }
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
                    milliseconds: 0.0,
                    error: Some("Runner stopped unexpectedly".into()),
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
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for BehaviorRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[macro_export]
macro_rules! laboratory_cases {
    (@add $cases:ident;) => {};
    (@add $cases:ident; $(#[$meta:meta])* async $name:ident, $($rest:tt)*) => {
        $(#[$meta])*
        $cases.push($crate::laboratory::Case {
            name: concat!(module_path!(), "::", stringify!($name)),
            run: |_runtime| {
                #[cfg(test)]
                $name();
                #[cfg(not(test))]
                _runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(30), $name())
                        .await.expect("behavior check timed out");
                });
            },
        });
        $crate::laboratory_cases!(@add $cases; $($rest)*);
    };
    (@add $cases:ident; $(#[$meta:meta])* $name:ident, $($rest:tt)*) => {
        $(#[$meta])*
        $cases.push($crate::laboratory::Case {
            name: concat!(module_path!(), "::", stringify!($name)),
            run: |_| $name(),
        });
        $crate::laboratory_cases!(@add $cases; $($rest)*);
    };
    ($($cases:tt)*) => {
        pub(crate) fn laboratory_cases(cases: &mut Vec<$crate::laboratory::Case>) {
            $crate::laboratory_cases!(@add cases; $($cases)*);
        }
    };
}

mod headless;
pub use headless::{HeadlessReport, run_headless};

mod tests;

pub(crate) fn isolate(world: &mut bevy::prelude::World) {
    world.insert_resource(crate::notifications::Notifications::new(
        cell::Diagnostics::default(),
    ));
}

pub(crate) mod resource_tests;
pub mod resources;
pub use resources::{
    GraphicsDevice, ResourceSnapshot, SandResources, StartupFailure, StartupIssue,
};
