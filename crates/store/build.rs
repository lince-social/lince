#[path = "build_support/migration_guard.rs"]
mod migration_guard;

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=migrations.sha384");
    println!("cargo:rerun-if-changed=build_support/migration_guard.rs");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    migration_guard::verify(std::path::Path::new(&manifest_dir))
        .unwrap_or_else(|error| panic!("Migration integrity check failed: {error}"));
    sensei::teach(manifest_dir);
}
