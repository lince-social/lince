#[path = "src/rules/mod.rs"]
#[allow(dead_code)]
mod rules;

#[path = "src/lesson.rs"]
#[allow(dead_code)]
mod lesson;

fn main() {
    println!("cargo:rerun-if-env-changed=SENSEI");
    let manifest =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    if std::env::var("SENSEI").as_deref() == Ok("off") {
        println!(
            "cargo:warning=SENSEI=off: the lessons did not run. \
             Run `cargo run -p sensei --bin sensei -- fix` to mend, then build without it."
        );
        return;
    }
    lesson::teach(&manifest);
}
