// non-lean-yet
// Scope: Expose the xtask target-cache budget pass as a standalone CLI.
// BeautifulRust (baseline lines):
// - [manual: §3 Ownership, §6 Idioms] Sites 5–13. The command collects remaining OS
//   arguments for a borrowed-slice API. Let the shared parser consume an iterator only
//   if both entry points preserve option validation, usage, and exit statuses.

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
