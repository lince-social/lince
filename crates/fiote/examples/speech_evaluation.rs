use fiote::speech::evaluation::{self, Suite};
use std::path::PathBuf;

const USAGE: &str = "Speech model evaluation (local Rust CPU inference)
  fetch <suite.json>                     Download and verify pinned model artifacts.
  run <suite.json> <new-output-directory> Compare all configured models offline.

Starter suite: crates/fiote/evaluations/english.json
Paths resolve relative to the suite file. Edit model directories to reuse existing assets.
Supports Phonon and Moonshine Streaming checkpoints with config, tokenizer and safetensors.
Compatible Moonshine Streaming variants require only suite configuration.
Phonon uses its registered checkpoint. New architectures implement Backend and register a loader.
Reports retain raw text, errors, word edits, cold load, latency, CPU RSS and timeouts.
Mark critical fixtures require_exact=true to enforce their expected words independently of aggregate WER.
All inputs are checksum-verified PCM16 mono 16 kHz WAV, up to 30 seconds each.
No account, microphone, production database, Fiote routing or UI is exercised.";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next().ok_or(USAGE)?;
    if command == "--help" || command == "help" {
        println!("{USAGE}");
        return Ok(());
    }
    let path = PathBuf::from(
        arguments
            .next()
            .ok_or("Choose an evaluation suite JSON file.")?,
    )
    .canonicalize()
    .map_err(|error| error.to_string())?;
    let suite = Suite::read(&path)?;
    if command == "fetch" {
        if arguments.next().is_some() {
            return Err("Unexpected fetch arguments.".into());
        }
        evaluation::fetch(&suite).await
    } else if command == "run" {
        let output = PathBuf::from(arguments.next().ok_or("Choose a new report directory.")?);
        if arguments.next().is_some() {
            return Err("Unexpected run arguments.".into());
        }
        let worker = std::env::current_exe().map_err(|error| error.to_string())?;
        let report = evaluation::run(suite, &output, &worker)?;
        if report.passed {
            Ok(())
        } else {
            Err(format!(
                "Evaluation failed. Results are in {}.",
                output.join("report.json").display()
            ))
        }
    } else if command == "worker" {
        let index = arguments
            .next()
            .ok_or("Missing model index.")?
            .to_str()
            .ok_or("Invalid model index.")?
            .parse()
            .map_err(|_| "Invalid model index.")?;
        let output = PathBuf::from(arguments.next().ok_or("Missing worker report path.")?);
        if arguments.next().is_some() {
            return Err("Unexpected worker arguments.".into());
        }
        evaluation::worker(&suite, index, &output)
    } else {
        Err("Use fetch <suite.json> or run <suite.json> <new-output-directory>.".into())
    }
}
