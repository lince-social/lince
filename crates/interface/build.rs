fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    if std::env::var_os("CARGO_FEATURE_NATIVE_RUNTIME").is_none() {
        return;
    }
    let names = [
        "minus",
        "plus",
        "rotate-ccw",
        "crosshair",
        "bring-to-front",
        "paintbrush",
        "x",
        "check",
        "layers",
        "palette",
        "store",
        "save",
        "pencil",
        "type",
        "text-cursor-input",
        "arrow-down-to-line",
        "move-vertical",
        "circle",
        "square",
        "info",
        "bell",
        "pin",
        "forward",
        "backward",
        "back",
        "delete",
        "general",
        "group",
        "ungroup",
        "attract",
        "repel",
        "play",
        "stop",
        "previous",
        "next",
        "person",
        "engine",
        "credits",
    ];
    let height = names.len().div_ceil(5) as u32 * 128;
    let mut sources = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let directory = if index < 21 { "lucide" } else { "lince" };
        let path = format!("../../assets/icons/{directory}/{name}.svg");
        println!("cargo:rerun-if-changed={path}");
        sources.push(std::fs::read_to_string(path).expect("read icon"));
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let atlas_path = output.join("icons.rgba");
    println!("cargo:rerun-if-changed={}", atlas_path.display());
    let inputs = [
        include_str!("build.rs"),
        include_str!("../../Cargo.lock"),
        &sources.join("\0"),
    ]
    .join("\0");
    let stamp = output.join("icons.inputs");
    if std::fs::read_to_string(&stamp).ok().as_deref() == Some(&inputs)
        && std::fs::metadata(&atlas_path)
            .is_ok_and(|file| file.len() == 640 * u64::from(height) * 4)
    {
        return;
    }
    if stamp.exists() {
        std::fs::remove_file(&stamp).expect("invalidate icon inputs");
    }
    let mut atlas = resvg::tiny_skia::Pixmap::new(640, height).unwrap();
    for (index, source) in sources.iter().enumerate() {
        let source = source.replace("currentColor", "white");
        let tree = resvg::usvg::Tree::from_data(source.as_bytes(), &Default::default())
            .expect("parse icon");
        let transform = resvg::tiny_skia::Transform::from_scale(128.0 / 24.0, 128.0 / 24.0)
            .post_translate((index % 5 * 128) as f32, (index / 5 * 128) as f32);
        resvg::render(&tree, transform, &mut atlas.as_mut());
    }
    let mut pixels = atlas.take();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[..3].fill(255);
    }
    std::fs::write(atlas_path, pixels).expect("write icon atlas");
    std::fs::write(stamp, inputs).expect("write icon inputs");
}
