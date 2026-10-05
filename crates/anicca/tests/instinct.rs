use anicca::instinct::{Requirement, validate};

#[test]
fn all_missing_requirements_are_reported_together() {
    let errors = validate(
        "",
        &[
            Requirement {
                slug: "protein",
                parent: None,
                order: None,
            },
            Requirement {
                slug: "step-one",
                parent: Some("protein"),
                order: None,
            },
        ],
    )
    .unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].message.contains("@protein"));
    assert!(errors[1].message.contains("@step-one"));
}

#[test]
fn parser_and_semantics_reject_invalid_bundles() {
    let required = &[Requirement {
        slug: "step",
        parent: Some("subject"),
        order: None,
    }];
    let errors = validate("Subject (@subject: 0, #instinct, #part-of @step) {\nText.\n}\nStep (@step: 0, #part-of @subject) {\n}\n", required).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("explanation"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("#instinct"))
    );
    assert!(errors.iter().any(|error| error.message.contains("cycle")));
    assert!(validate("Broken (@broken: 0) {", required).is_err());
}

#[test]
fn steps_require_the_declared_subject_and_unique_slugs() {
    let source = "Subject (@subject: 0, #instinct) {\nText.\n}\nStep (@step: 0, #instinct) {\nInstruction.\n}\nDuplicate (@step: 0) {\nText.\n}\n";
    let errors = validate(
        source,
        &[Requirement {
            slug: "step",
            parent: Some("subject"),
            order: None,
        }],
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("Duplicate slug"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("#part-of @subject"))
    );
}

#[test]
fn chapter_and_page_order_must_match_the_manifest() {
    let source = "Handbook (@handbook: 0, #instinct) {\nText.\n}\nChapter (@chapter: 0, is #chapter, #instinct, #part-of @handbook: 2) {\nText.\n}\nPage (@page: 0, #instinct, #part-of @chapter: 2) {\nText.\n}\n";
    let errors = validate(
        source,
        &[
            Requirement {
                slug: "chapter",
                parent: Some("handbook"),
                order: Some(1),
            },
            Requirement {
                slug: "page",
                parent: Some("chapter"),
                order: Some(1),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        errors
            .iter()
            .filter(|error| error.message.contains("teaching order"))
            .count(),
        2
    );
}
