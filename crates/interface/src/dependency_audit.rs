use lince_interface::dependency_graph::DependencyAudit;

fn main() {
    let audit = match DependencyAudit::collect() {
        Ok(audit) => audit,
        Err(error) => {
            eprintln!("Lince Interface dependency audit failed: {error}");
            std::process::exit(1);
        }
    };
    println!("{}", audit.human_summary());
    match audit.write_default() {
        Ok(path) => println!("REPORT  {}", path.display()),
        Err(error) => {
            eprintln!("Dependency report export failed: {error}");
            std::process::exit(1);
        }
    }
}
