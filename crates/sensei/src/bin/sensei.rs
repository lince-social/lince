use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = sensei::workspace_root(&manifest);
    let sources = sensei::workspace_sources(&root);

    match std::env::args().nth(1).as_deref() {
        Some("fix") => {
            let mended = match sensei::mend(&sources) {
                Ok(mended) => mended,
                Err(error) => {
                    eprintln!("sensei could not rewrite a file: {error}");
                    std::process::exit(2);
                }
            };
            if mended.is_empty() {
                println!("sensei: nothing to mend across {} files", sources.len());
                return;
            }
            for item in &mended {
                println!("mended {}", item.path.display());
            }
            println!("sensei: {} files mended", mended.len());
            println!(
                "sensei: run `cargo fmt` next; a removed line can leave a signature short enough to rejoin."
            );
        }
        Some("check") | None => match sensei::examine(&sources) {
            None => println!("sensei: {} files, nothing to say", sources.len()),
            Some(complaint) => {
                eprintln!("{complaint}");
                std::process::exit(1);
            }
        },
        Some(other) => {
            eprintln!("sensei: no such lesson `{other}`. Try `check` or `fix`.");
            std::process::exit(2);
        }
    }
}
