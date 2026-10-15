fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        lince_park::runtime::probe()?;
        println!(
            "Park worker: transformed pixels, retained buffers through remap, modal input, and shutdown verified."
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err("Park requires Linux".into())
}
