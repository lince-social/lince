use super::{CaseResult, StressConfig, StressReport, catalogue, stress::StressRun};
use bevy::{prelude::*, time::TimeUpdateStrategy};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct HeadlessReport {
    pub schema: u32,
    pub behavior: Vec<CaseResult>,
    pub stress: StressReport,
}

pub fn run_headless(config: StressConfig) -> std::io::Result<HeadlessReport> {
    if !config.validate() {
        return Err(std::io::Error::other("Invalid Laboratory stress limits"));
    }
    std::thread::Builder::new()
        .name("laboratory-headless".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(32 * 1024 * 1024)
                .enable_all()
                .build()?;
            let behavior = catalogue()
                .into_iter()
                .map(|case| {
                    let result = case.execute(&runtime);
                    eprintln!(
                        "{} {} ({:.1} ms)",
                        if result.error.is_none() {
                            "PASS"
                        } else {
                            "FAIL"
                        },
                        result.name,
                        result.milliseconds
                    );
                    result
                })
                .collect();
            let (mut app, root) = stress_app();
            app.finish();
            app.cleanup();
            let mut run = StressRun::new(config, false, Vec2::new(1000.0, 800.0));
            let mut elapsed = 0.0;
            while !run.report.complete {
                let start = Instant::now();
                run.advance(app.world_mut(), root, elapsed);
                app.update();
                elapsed = start.elapsed().as_secs_f64() * 1000.0;
            }
            Ok(HeadlessReport {
                schema: 2,
                behavior,
                stress: run.report,
            })
        })?
        .join()
        .map_err(|_| std::io::Error::other("Laboratory headless runner panicked"))?
}

pub(crate) fn stress_app() -> (App, Entity) {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .add_plugins((
            MinimalPlugins,
            crate::theme::ThemePlugin,
            crate::sand::SandPlugin,
            crate::sand_text::SandTextPlugin,
            crate::effect::EffectPlugin,
            crate::physics::WorkspacePhysicsPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
            16_666_667,
        )));
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
            super::LaboratoryRoot,
        ))
        .id();
    (app, root)
}
