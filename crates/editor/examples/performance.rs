// non-lean-yet
// Scope: fn memory, fn main.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 43, 44.
// - [Clippy: clippy::panic_in_result_fn] Return the existing error type for recoverable failure.
//   Sites 16.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   45.
// - [Clippy: clippy::missing_assert_message] assert without any message. Sites 33, 51, 64, 68.

use std::time::Instant;

use lince_editor::{Buffer, Edit, MAX_WINDOW_BYTES};

fn memory() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

fn main() -> lince_editor::Result<()> {
    println!("Before opening: {}", memory());
    let line = "let plain_text = \"a Unicode line: 猫\";\n";
    let text = line.repeat((10_usize * 1024 * 1024).div_ceil(line.len()));
    let opened = Instant::now();
    let mut buffer = Buffer::new(&text)?;
    let open_time = opened.elapsed();
    let first = buffer.snapshot().len_lines() / 2;
    let mut samples = Vec::new();
    for _ in 0..300 {
        let start = buffer.snapshot().line_to_char(first);
        let begin = Instant::now();
        buffer.edit(Edit {
            range: start..start,
            text: "x".into(),
        })?;
        let window = buffer.window(first, 96);
        assert!(window.text.len() <= MAX_WINDOW_BYTES);
        samples.push(begin.elapsed());
        buffer.edit(Edit {
            range: start..start + 1,
            text: String::new(),
        })?;
    }
    samples.sort_unstable();
    println!(
        "10 MiB buffer: open {open_time:?}; edit + 96-line projection p50 {:?}, p95 {:?}, max {:?}",
        samples[150],
        samples[285],
        samples.last().expect("the benchmark collected 300 samples")
    );
    println!("This measures the buffer, not GPU text layout or application idle CPU.");
    let begin = Instant::now();
    let point = buffer.prepare_save()?;
    println!("Save snapshot preparation: {:?}", begin.elapsed());
    assert_eq!(point.text.len_bytes(), text.len());
    let begin = Instant::now();
    let point = point.encode()?;
    println!("Worker save encoding: {:?}", begin.elapsed());
    let saved_text = point.text.to_string().into();
    let begin = Instant::now();
    buffer.saved_with_text(point, saved_text)?;
    println!("Save acknowledgement: {:?}", begin.elapsed());
    let changed = format!("{text}external line\n");
    let begin = Instant::now();
    let plan = buffer.reconciliation().compute(changed)?;
    println!("Worker disk comparison: {:?}", begin.elapsed());
    let begin = Instant::now();
    assert!(buffer.accept(plan)?);
    println!("Disk reconciliation on UI: {:?}", begin.elapsed());
    let pattern = lince_editor::search::Pattern::new("missing needle", Default::default())?;
    let begin = Instant::now();
    assert!(
        pattern
            .find(&buffer.snapshot(), 0, false, &Default::default())?
            .is_none()
    );
    println!("Full-buffer missing-text search: {:?}", begin.elapsed());
    let before = memory();
    let begin = Instant::now();
    for _ in 0..10_000 {
        buffer.edit(Edit {
            range: 0..0,
            text: "x".into(),
        })?;
        buffer.edit(Edit {
            range: 0..1,
            text: String::new(),
        })?;
    }
    println!(
        "20,000 further edits: {:?}; before {}; after {}",
        begin.elapsed(),
        before,
        memory()
    );
    Ok(())
}
