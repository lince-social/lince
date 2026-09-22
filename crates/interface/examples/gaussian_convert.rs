use lince_interface::topology::splats::codec::convert_ply;
use std::{io, path::PathBuf};

fn main() -> io::Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err(io::Error::other(
            "Usage: gaussian_convert input.ply output.gcloud",
        ));
    }
    let source = PathBuf::from(&arguments[0]);
    let output = PathBuf::from(&arguments[1]);
    let (count, bytes) = convert_ply(&source, &output)?;
    println!(
        "Converted {count} splats to {} ({bytes} bytes). Detail and coordinates are preserved. Keep the source license beside the converted asset.",
        output.display()
    );
    Ok(())
}
