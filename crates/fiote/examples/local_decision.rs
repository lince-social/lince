use fiote::communication::decision::{
    Input, Model,
    laya::{Bundle, Driver, Thresholds},
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments
        .next()
        .ok_or("Use install <model-cache> [directory] or evaluate <model-cache> <requests.json>")?;
    let root = PathBuf::from(arguments.next().ok_or("Choose an explicit model cache.")?);
    let cancelled = Arc::new(AtomicBool::new(false));
    if command == "install"
        || command == "install-nli"
        || command == "install-nli-modern"
        || command == "install-qwen"
    {
        let source = arguments.next().map(PathBuf::from);
        if arguments.next().is_some() {
            return Err("Too many installation arguments.".into());
        }
        if command == "install-qwen" {
            let driver = fiote::communication::decision::qwen::Driver::new(root);
            driver
                .acquire(source, cancelled, Arc::new(AtomicU64::new(0)))
                .await?;
            println!(
                "{}",
                serde_json::json!({"information":driver.information(),"present":driver.present()})
            );
            return Ok(());
        }
        if command == "install-nli" || command == "install-nli-modern" {
            let driver = if command == "install-nli-modern" {
                fiote::communication::decision::nli::Driver::modern(root)
            } else {
                fiote::communication::decision::nli::Driver::new(root)
            };
            driver
                .acquire(source, cancelled, Arc::new(AtomicU64::new(0)))
                .await?;
            println!(
                "{}",
                serde_json::json!({"directory":driver.directory()?,"information":driver.information(),"present":driver.present()})
            );
            return Ok(());
        }
        let bundle = Bundle::new(root);
        bundle
            .acquire(source, cancelled, Arc::new(AtomicU64::new(0)))
            .await?;
        println!(
            "{}",
            serde_json::json!({"directory":bundle.directory()?,"bytes":bundle.required_bytes(),"present":bundle.present()})
        );
    } else if command == "evaluate"
        || command == "evaluate-raw"
        || command == "evaluate-binary-raw"
        || command == "evaluate-nli"
        || command == "evaluate-nli-raw"
        || command == "evaluate-nli-modern"
        || command == "evaluate-qwen"
    {
        let source = arguments.next().ok_or("Choose a request fixture JSON.")?;
        if arguments.next().is_some() {
            return Err("Too many evaluation arguments.".into());
        }
        let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
        if bytes.len() > 1_048_576 {
            return Err("Decision fixture exceeds 1 MiB.".into());
        }
        let inputs: Vec<Input> =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if inputs.is_empty() || inputs.len() > 256 {
            return Err("Choose 1–256 decision fixtures.".into());
        }
        let thresholds = if command == "evaluate-raw" || command == "evaluate-binary-raw" {
            Thresholds {
                admission_probability: 0.0,
                routing_probability: 0.0,
                margin: 0.0,
                act_probability: 0.0,
            }
        } else {
            Thresholds::default()
        };
        let diagnostic = Arc::new(Mutex::new(None));
        let driver: Box<dyn Model> = if command == "evaluate-qwen" {
            Box::new(
                fiote::communication::decision::qwen::Driver::new(root)
                    .with_diagnostic(diagnostic.clone()),
            )
        } else if command == "evaluate-nli"
            || command == "evaluate-nli-raw"
            || command == "evaluate-nli-modern"
        {
            let mut driver = if command == "evaluate-nli-modern" {
                fiote::communication::decision::nli::Driver::modern(root)
            } else {
                fiote::communication::decision::nli::Driver::new(root)
            };
            if command == "evaluate-nli-raw" {
                driver.admission_threshold = 0.0;
                driver.routing_threshold = 0.0;
                driver.margin_threshold = 0.0;
            }
            Box::new(driver)
        } else {
            let driver = Driver::new(root, thresholds);
            Box::new(if command == "evaluate-binary-raw" {
                driver.with_binary_admission()
            } else {
                driver
            })
        };
        let mut failures = 0;
        for input in inputs {
            let started = std::time::Instant::now();
            let output = match driver.decide(&input, &cancelled) {
                Ok(output) => output,
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::json!({"id":input.request_id,"error":error,"elapsed_ms":started.elapsed().as_millis(),"diagnostic":diagnostic.lock().map_err(|_| "Diagnostic collector failed.")?.take()})
                    );
                    failures += 1;
                    continue;
                }
            };
            let memory = std::fs::read_to_string("/proc/self/status")
                .ok()
                .map(|status| {
                    status
                        .lines()
                        .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                });
            println!(
                "{}",
                serde_json::json!({"id":input.request_id,"result":output,"memory":memory})
            );
        }
        if failures > 0 {
            return Err(format!(
                "{failures} decision requests failed; retained every available result."
            ));
        }
    } else {
        return Err("Use install or evaluate.".into());
    }
    Ok(())
}
