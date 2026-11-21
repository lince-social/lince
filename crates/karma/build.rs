use std::env::{VarError, var};

fn main() -> Result<(), VarError> {
    sensei::teach(var("CARGO_MANIFEST_DIR")?);
    Ok(())
}
