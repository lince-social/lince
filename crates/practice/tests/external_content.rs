use std::{path::PathBuf, process::Command};

#[test]
fn handbook_and_practice_are_external_and_optional() {
    if let Ok(mode) = std::env::var("LINCE_CONTENT_TEST_MODE") {
        let pages = lince_practice::handbook::pages();
        let lessons = lince_practice::practice::lessons();
        if mode == "absent" {
            assert!(pages.is_empty());
            assert!(lessons.is_empty());
            assert!(lince_practice::content::publication().unwrap().is_none());
        } else {
            assert!(pages.len() > 20);
            assert!(!lessons.is_empty());
            assert!(lince_practice::content::publication().unwrap().is_some());
            assert_eq!(
                lince_practice::content::start("area-forces"),
                Some(("areas-of-influence", 1))
            );
        }
        return;
    }
    let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let configuration =
        std::env::temp_dir().join(format!("lince-content-test-{}", std::process::id()));
    std::fs::create_dir_all(&configuration).unwrap();
    for (mode, location) in [
        ("absent", String::new()),
        ("folder", content.to_string_lossy().into_owned()),
        (
            "file",
            content
                .join("instinct/Instinct.lingua")
                .to_string_lossy()
                .into_owned(),
        ),
    ] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "handbook_and_practice_are_external_and_optional",
                "--nocapture",
            ])
            .env("LINCE_CONTENT_TEST_MODE", mode)
            .env("LINCE_CONTENT", location)
            .env("XDG_CONFIG_HOME", &configuration)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(configuration).unwrap();
}
