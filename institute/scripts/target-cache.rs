#[path = "../../crates/xtask/src/prune/budget.rs"]
mod budget;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let Some(target) = arguments.next() else {
        eprintln!("usage: lince-target-cache TARGET [--dry-run] [--max-bytes BYTES]");
        std::process::exit(2);
    };
    if let Err(error) = budget::command(
        std::path::Path::new(&target),
        &arguments.collect::<Vec<_>>(),
    ) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
