mod build_freedoom;
mod icon_art;

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_NATIVE_MEDIA").is_some()
    {
        let path =
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
                .join("../lince/macos/Info.plist");
        println!("cargo:rerun-if-changed={}", path.display());
        println!(
            "cargo:rustc-link-arg-examples=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            path.display()
        );
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_RUNTIME").is_none() {
        return;
    }
    build_freedoom::compile();
    println!("cargo:rerun-if-changed=icon_art.rs");
    let height = icon_art::COUNT.div_ceil(5) as u32 * 128;
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let atlas_path = output.join("icons.rgba");
    let inputs = [include_str!("build.rs"), include_str!("icon_art.rs")].join("\0");
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
    for index in 0..icon_art::COUNT {
        let source = icon_art::source(index).replace("currentColor", "white");
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
