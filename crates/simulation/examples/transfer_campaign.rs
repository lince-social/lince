fn main() -> simulation::Result<()> {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    let args: Vec<_> = std::env::args().skip(1).collect();
                    let output = std::path::Path::new(
                        args.first()
                            .ok_or("expected output directory and optional case count")?,
                    );
                    let count = args
                        .get(1)
                        .map(|count| count.parse())
                        .transpose()?
                        .unwrap_or(46);
                    let status = simulation::campaign::run(output, Some(count)).await?;
                    if status != 0 {
                        return Err(format!("campaign stopped with status {status}").into());
                    }
                    Ok(())
                })
        })?
        .join()
        .map_err(|_| "campaign runner panicked")?
}
