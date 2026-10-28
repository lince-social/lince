use lince_resources::{Library, Locator, MediaInput, Resource, SourceServer, video};
use std::{
    fs,
    process::{Command, Stdio},
};

#[test]
#[ignore = "Requires FFmpeg from the native Nix development shell"]
fn video_previews_and_retained_intervals_work_without_the_original_file() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("original.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=32x32:rate=8",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=16000",
            "-t",
            "1",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
        ])
        .arg(&path)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let library = Library::new(temporary.path().join("library"));
    let resource = Resource {
        uri: lince_resources::address(path.to_str().unwrap())
            .unwrap()
            .to_string(),
        media_type: "video/x-matroska".into(),
        locator: Locator {
            start_ms: Some(250),
            end_ms: Some(750),
            ..Default::default()
        },
        ..Default::default()
    };
    let input = MediaInput::open(&library, &resource).unwrap();
    assert_eq!(input.uri, path.to_string_lossy());
    assert!(SourceServer::start(library.clone(), &resource).is_err());
    let pixels = video::preview(&library, &resource, 64, 64).unwrap();
    assert_eq!(pixels.len(), 64 * 64 * 4);
    let retained = video::retain(&library, &resource).unwrap();
    assert!(retained.bytes > 0);
    assert_eq!(
        video::retain(&library, &resource).unwrap().path,
        retained.path
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        video::retained(&library, &resource).unwrap().unwrap().path,
        retained.path
    );
    assert_eq!(
        video::preview(&library, &resource, 64, 64).unwrap().len(),
        64 * 64 * 4
    );
}
