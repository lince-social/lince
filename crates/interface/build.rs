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
    ];
    let mut atlas = resvg::tiny_skia::Pixmap::new(640, 768).unwrap();
    for (index, name) in names.iter().enumerate() {
        let directory = if index < 21 { "lucide" } else { "lince" };
        let path = format!("../../assets/icons/{directory}/{name}.svg");
        println!("cargo:rerun-if-changed={path}");
        let source = std::fs::read_to_string(path)
            .expect("read icon")
            .replace("currentColor", "white");
        let tree = resvg::usvg::Tree::from_data(source.as_bytes(), &Default::default())
            .expect("parse icon");
        let transform = resvg::tiny_skia::Transform::from_scale(128.0 / 24.0, 128.0 / 24.0)
            .post_translate((index % 5 * 128) as f32, (index / 5 * 128) as f32);
        resvg::render(&tree, transform, &mut atlas.as_mut());
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let mut pixels = atlas.take();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[..3].fill(255);
    }
    std::fs::write(output.join("icons.rgba"), pixels).expect("write icon atlas");
}
