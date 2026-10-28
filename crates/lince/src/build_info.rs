use std::io;

pub fn print() -> io::Result<()> {
    let information = serde_json::json!({
        "version": utils::build_info::VERSION,
        "revision": utils::build_info::revision(),
        "target": utils::build_info::target_triple(),
        "optimized": !cfg!(debug_assertions),
        "content": {"embedded": false, "delivery": "external"},
        "features": {
            "ui": cfg!(feature = "ui"),
            "instinct": cfg!(feature = "instinct"),
            "park": cfg!(all(target_os = "linux", feature = "park"))
        }
    });
    serde_json::to_writer(io::stdout().lock(), &information).map_err(io::Error::other)?;
    println!();
    Ok(())
}
