fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").unwrap());
}
