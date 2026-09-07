fn main() {
    println!("cargo:rerun-if-env-changed=LINCE_REVISION");
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
}
