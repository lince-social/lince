#[path = "src/rules/mod.rs"]
mod rules;

#[path = "src/lesson.rs"]
mod lesson;

fn main() {
    lesson::teach(env!("CARGO_MANIFEST_DIR"));
}
