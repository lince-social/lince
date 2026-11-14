use lince_interface::{host::DesktopAction, native::app::CellHandle};
use lince_park::runtime::{CanvasWorker, Completed, Configuration, Input};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn wait(worker: &CanvasWorker, serial: u64, deadline: Instant) -> Result<Completed, String> {
    loop {
        if let Some(error) = worker.failure() {
            return Err(error.to_string());
        }
        if let Some(frame) = worker.frame()
            && frame.input_serial >= serial
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Input {serial} did not reach a completed frame within its deadline"
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--build-info") {
        println!(
            "{}",
            serde_json::json!({
                "probe": "park-responsiveness-v1",
                "package": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION"),
                "revision": option_env!("LINCE_REVISION").unwrap_or("unknown"),
                "optimized": !cfg!(debug_assertions),
                "architecture": std::env::consts::ARCH,
                "os": std::env::consts::OS,
            })
        );
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--suite") {
        return std::thread::Builder::new()
            .name("park-suite".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(lince_park::responsiveness::run)?
            .join()
            .map_err(|_| "Suite panicked")?
            .map_err(Into::into);
    }
    if std::env::args().nth(1).as_deref() == Some("--interface") {
        let results = lince_interface::native::laboratory::run_interface_checks()?;
        return if results.iter().all(|result| result.error.is_none()) {
            Ok(())
        } else {
            Err("Interface checks failed".into())
        };
    }
    if std::env::args().nth(1).as_deref() == Some("--projection-reveal") {
        return std::thread::Builder::new()
            .name("park-projection-reveal".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(lince_park::responsiveness::projection_reveal)?
            .join()
            .map_err(|_| "Projection reveal probe panicked")?
            .map_err(Into::into);
    }
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass a private fixture directory")?,
    )
    .canonicalize()?;
    if std::fs::read_to_string(directory.join("PRIVATE-PERFORMANCE-FIXTURE"))?
        != "isolated; no services or credentials\n"
    {
        return Err("This command only opens explicitly prepared private fixtures".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()?;
    let engine = Arc::new(runtime.block_on(engine::Engine::open(&format!(
        "sqlite://{}",
        directory.join("lince.db").display()
    )))?);
    let cell = CellHandle(cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.clone()),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
        speech: None,
    });
    let started = Instant::now();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [1920, 1080],
        directory,
        runtime: runtime.handle().clone(),
        cell: Some(cell),
    })?;
    let frame = wait(&worker, 0, started + Duration::from_secs(120))?;
    println!(
        "{}",
        serde_json::json!({"phase":"startup", "milliseconds":started.elapsed().as_secs_f64()*1000.0,"controls":frame.frame.input.control.commands.len()})
    );
    drop(frame);
    for round in 0..12 {
        while worker.frame().is_some() {}
        let started = Instant::now();
        let serial = worker
            .input(Input::Action(DesktopAction::ToggleEdit))
            .map_err(|_| "Input refused")?;
        let frame = wait(&worker, serial, started + Duration::from_secs(90))?;
        println!(
            "{}",
            serde_json::json!({"phase":"toggle-edit", "round":round,"milliseconds":started.elapsed().as_secs_f64()*1000.0,"controls":frame.frame.input.control.commands.len(),"input_serial":frame.input_serial})
        );
        drop(frame);
        std::thread::sleep(Duration::from_millis(100));
    }
    let soak = Instant::now();
    let mut round = 0;
    while soak.elapsed() < Duration::from_secs(30) {
        let started = Instant::now();
        let serial = worker
            .input(Input::Action(DesktopAction::ToggleEdit))
            .map_err(|_| "Input refused")?;
        let frame = wait(&worker, serial, started + Duration::from_secs(30))?;
        println!(
            "{}",
            serde_json::json!({"phase":"soak-toggle", "round":round,"milliseconds":started.elapsed().as_secs_f64()*1000.0,"controls":frame.frame.input.control.commands.len(),"input_serial":frame.input_serial})
        );
        round += 1;
        std::thread::sleep(Duration::from_millis(250));
    }
    worker.stop()?;
    if !lince_interface::host::performance::finish(Duration::from_secs(3)) {
        return Err("Performance report did not finish".into());
    }
    Ok(())
}
