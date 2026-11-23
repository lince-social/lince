// non-lean-yet
// Scope: fn handbook_and_practice_are_external_and_optional.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

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
            let publication = lince_practice::content::publication().unwrap().unwrap();
            let steps: std::collections::BTreeMap<_, _> = publication
                .documents
                .iter()
                .flat_map(|document| &document.project.records)
                .filter_map(|record| Some((record.slug.as_deref()?, record.body.trim())))
                .collect();
            for step in lessons
                .iter()
                .chain(lince_practice::handbook::reading())
                .flat_map(|lesson| lesson.steps)
            {
                assert!(
                    steps.get(step.slug).is_some_and(|body| !body.is_empty()),
                    "{} needs an instruction Record",
                    step.slug
                );
            }
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
    let installed = configuration.join("lince/content");
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::copy(content.join("Lince.lingua"), installed.join("Lince.lingua")).unwrap();
    for (mode, location) in [
        ("absent", Some(String::new())),
        ("folder", Some(content.to_string_lossy().into_owned())),
        (
            "file",
            Some(content.join("Lince.lingua").to_string_lossy().into_owned()),
        ),
        ("installed", None),
    ] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "handbook_and_practice_are_external_and_optional",
                "--nocapture",
            ])
            .env("LINCE_CONTENT_TEST_MODE", mode)
            .env("XDG_CONFIG_HOME", &configuration);
        if let Some(location) = location {
            child.env("LINCE_CONTENT", location);
        } else {
            child.env_remove("LINCE_CONTENT");
        }
        let result = child.output().unwrap();
        assert!(
            result.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(configuration).unwrap();
}
