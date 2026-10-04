use std::{collections::BTreeSet, path::Path};

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_INSTINCT");
    let mut output = String::from("pub static BUNDLE: &[(&str, &str)] = &[\n");
    let mut manifest = BTreeSet::new();
    if std::env::var_os("CARGO_FEATURE_INSTINCT").is_some() {
        let source = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
            .join("../../institute/anicca/Lince.lingua");
        println!("cargo:rerun-if-changed={}", source.display());
        let text = std::fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
        let mut requirements = Vec::new();
        let mut seen = BTreeSet::new();
        let mut steps = std::collections::BTreeMap::new();
        for page in lince_interface::handbook::PAGES {
            assert!(
                seen.insert(page.slug),
                "Duplicate handbook manifest slug @{}",
                page.slug
            );
            requirements.push(anicca::instinct::Requirement {
                slug: page.slug,
                parent: Some(page.chapter),
            });
            manifest.insert(page.chapter);
            manifest.insert(page.slug);
        }
        for lesson in lince_interface::practice::LESSONS
            .iter()
            .chain(lince_interface::handbook::READING)
        {
            requirements.push(anicca::instinct::Requirement {
                slug: lesson.subject,
                parent: None,
            });
            manifest.insert(lesson.subject);
            for step in lesson.steps {
                if step.slug == lesson.subject {
                    continue;
                }
                if let Some(previous) = steps.insert(step.slug, *step) {
                    assert_eq!(previous, *step, "Conflicting step manifest @{}", step.slug);
                    continue;
                }
                assert!(
                    seen.insert(step.slug),
                    "Duplicate step manifest slug @{}",
                    step.slug
                );
                requirements.push(anicca::instinct::Requirement {
                    slug: step.subject,
                    parent: None,
                });
                requirements.push(anicca::instinct::Requirement {
                    slug: step.slug,
                    parent: Some(step.subject),
                });
                manifest.insert(step.subject);
                manifest.insert(step.slug);
            }
        }
        manifest.insert("instinct-handbook");
        for slug in manifest.iter().filter(|slug| {
            **slug == "instinct-handbook"
                || lince_interface::handbook::PAGES
                    .iter()
                    .any(|page| page.chapter == **slug)
        }) {
            requirements.push(anicca::instinct::Requirement {
                slug,
                parent: if *slug == "instinct-handbook" {
                    None
                } else {
                    Some("instinct-handbook")
                },
            });
        }
        if let Err(errors) = anicca::instinct::validate(&text, &requirements) {
            let diagnostics = errors
                .into_iter()
                .map(|error| format!("{}: {error}", source.display()))
                .collect::<Vec<_>>()
                .join("\n");
            panic!("Instinct content validation failed:\n{diagnostics}");
        }
        output.push_str(&format!("    ({:?}, {text:?}),\n", "Lince.lingua"));
    }
    output.push_str("];\n");
    let manifest: Vec<_> = manifest.into_iter().collect();
    output.push_str(&format!("pub static MANIFEST: &[&str] = &{manifest:?};\n"));

    let destination = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("bundle.rs");
    if std::fs::read_to_string(&destination).ok().as_deref() != Some(output.as_str()) {
        std::fs::write(destination, output).expect("write bundle.rs");
    }
}
