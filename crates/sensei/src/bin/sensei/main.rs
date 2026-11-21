mod analyzer;
mod cleanup;
mod compiler;
mod lsp;

use std::{
    env::args_os,
    io,
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let root = sensei::workspace_root(&PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let mut args = args_os().skip(1);
    let command = args.next();
    match command.as_deref().and_then(|arg| arg.to_str()) {
        Some("fix") => report(cleanup::fix(&root, args)),
        Some("paths") => {
            let patterns: Vec<_> = args.collect();
            if patterns.is_empty() {
                return report(Err(io::Error::other(
                    "supply at least one rust-analyzer search pattern",
                )));
            }
            report(cleanup::run(
                Command::new("rust-analyzer")
                    .current_dir(&root)
                    .arg("search")
                    .args(patterns),
            ))
        }
        Some("check") | None
            if args.len() == 0 && command.as_deref().is_none_or(|command| command == "check") =>
        {
            let sources = sensei::workspace_sources(&root);
            if let Some(complaint) = sensei::examine(&sources) {
                eprintln!("{complaint}");
                return ExitCode::FAILURE;
            }
            println!("sensei: {} files, nothing to say", sources.len());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: sensei check | fix [--preview] [--no-comments] [--manifest-path PATH | --workspace] [--compiler | --compiler-only] [--shorten-paths] | paths PATTERN..."
            );
            ExitCode::from(2)
        }
    }
}

fn report(result: io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sensei: {error}");
            ExitCode::from(2)
        }
    }
}
