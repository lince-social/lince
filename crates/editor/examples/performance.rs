use lince_editor::{Buffer, Edit, MAX_WINDOW_BYTES};
use std::time::Instant;

fn main() {
    let line = "let plain_text = \"a Unicode line: 猫\";\n";
    let text = line.repeat((10_usize * 1024 * 1024).div_ceil(line.len()));
    let opened = Instant::now();
    let mut buffer = Buffer::new(&text).unwrap();
    let open_time = opened.elapsed();
    let first = buffer.snapshot().len_lines() / 2;
    let mut samples = Vec::new();
    for _ in 0..300 {
        let start = buffer.snapshot().line_to_char(first);
        let begin = Instant::now();
        buffer
            .edit(Edit {
                range: start..start,
                text: "x".into(),
            })
            .unwrap();
        let window = buffer.window(first, 96);
        assert!(window.text.len() <= MAX_WINDOW_BYTES);
        samples.push(begin.elapsed());
        buffer
            .edit(Edit {
                range: start..start + 1,
                text: String::new(),
            })
            .unwrap();
    }
    samples.sort_unstable();
    println!(
        "10 MiB buffer: open {open_time:?}; edit + 96-line projection p50 {:?}, p95 {:?}, max {:?}",
        samples[150],
        samples[285],
        samples.last().unwrap()
    );
    println!("This measures the buffer, not GPU text layout or application idle CPU.");
    let begin = Instant::now();
    let point = buffer.prepare_save().unwrap();
    println!("Save snapshot preparation: {:?}", begin.elapsed());
    assert_eq!(point.text.len_bytes(), text.len());
    let saved_text = point.text.to_string().into();
    let begin = Instant::now();
    buffer.saved_with_text(point, saved_text).unwrap();
    println!("Save acknowledgement: {:?}", begin.elapsed());
    let pattern = lince_editor::search::Pattern::new("missing needle", Default::default()).unwrap();
    let begin = Instant::now();
    assert!(
        pattern
            .find(&buffer.snapshot(), 0, false, &Default::default())
            .unwrap()
            .is_none()
    );
    println!("Full-buffer missing-text search: {:?}", begin.elapsed());
}
