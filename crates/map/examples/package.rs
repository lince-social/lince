fn main() -> Result<(), String> {
    let destination = std::env::args_os()
        .nth(1)
        .ok_or("Supply the directory containing the packaged Lince binary")?;
    lince_map::pack::package(std::path::Path::new(&destination))
}
