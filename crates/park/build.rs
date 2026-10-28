fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && std::env::var_os("CARGO_FEATURE_SESSION").is_some()
    {
        pkg_config::Config::new()
            .atleast_version("0.4.0")
            .probe("libdisplay-info")
            .expect("Park's physical session requires libdisplay-info >= 0.4.0");
    }
}
