fn main() {
    println!("cargo:rerun-if-changed=migrations");
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
}
