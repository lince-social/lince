use lince_map::{
    Settings, TileStore,
    region::{self, Progress},
};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err("Use: cargo xtask map-region <source-settings.json> <region.json> <new-output-directory>".into());
    }
    let settings: Settings = serde_json::from_slice(&std::fs::read(&arguments[0])?)?;
    let definition = region::read_definition(&PathBuf::from(&arguments[1]))?;
    let output = PathBuf::from(&arguments[2]);
    let cache = std::env::temp_dir().join("lince-region-cache");
    let store = TileStore::new(settings, cache)?;
    store.validate_sources().await?;
    let manifest = region::prepare(
        store,
        definition,
        output,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
    )
    .await?;
    println!("{}", manifest.identity()?);
    Ok(())
}
