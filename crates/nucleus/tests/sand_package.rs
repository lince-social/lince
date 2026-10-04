use nucleus::sand_package::{self as model, Identity, Kind, License, Manifest, Package};

fn package() -> Package {
    let mut package = Package {
        format: model::FORMAT.into(),
        manifest: Manifest {
            identity: Identity {
                origin: nucleus::new_uid("r"),
                id: nucleus::new_uid("r"),
                version: 1,
            },
            name: "Castle".into(),
            kind: Kind::Castle,
            author: nucleus::new_uid("r"),
            execution: "future.execution.v2".into(),
            permissions: vec!["camera".into()],
            licenses: vec![License {
                name: "MIT".into(),
                text: "Full license\nAuthor attribution".into(),
            }],
            credits: vec!["Original author".into()],
            key_id: "key1".into(),
            public_key: "public-key".into(),
        },
        payload: "Unrecognized payload".into(),
        digest: String::new(),
        signature: "signature".into(),
    };
    package.digest = package.content_digest().unwrap();
    package
}

#[test]
fn envelope_keeps_unknown_execution_metadata_and_checks_its_full_content_digest() {
    let original = package();
    original.validate().unwrap();
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: Package = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, original);
    for modified in [
        {
            let mut modified = original.clone();
            modified.manifest.author = nucleus::new_uid("r");
            modified
        },
        {
            let mut modified = original.clone();
            modified.manifest.licenses[0].text.clear();
            modified
        },
        {
            let mut modified = original.clone();
            modified.manifest.permissions.clear();
            modified
        },
        {
            let mut modified = original.clone();
            modified.payload.push('!');
            modified
        },
    ] {
        assert!(modified.validate().is_err());
    }
}

#[test]
fn package_validation_bounds_metadata_and_payload_and_requires_stable_identity() {
    for modified in [
        {
            let mut modified = package();
            modified.manifest.identity.version = 0;
            modified
        },
        {
            let mut modified = package();
            modified.manifest.identity.origin = "invented".into();
            modified
        },
        {
            let mut modified = package();
            modified.manifest.licenses.clear();
            modified
        },
        {
            let mut modified = package();
            modified.manifest.licenses[0].text = "x".repeat(65_537);
            modified
        },
        {
            let mut modified = package();
            modified.manifest.credits = vec!["credit".into(); 65];
            modified
        },
        {
            let mut modified = package();
            modified.payload = "x".repeat(nucleus::component::composition::MAX_BYTES + 1);
            modified
        },
    ] {
        assert!(modified.validate().is_err());
    }
    let mut json = serde_json::to_value(package()).unwrap();
    json["manifest"]["hidden_permissions"] = serde_json::json!(["shell"]);
    assert!(serde_json::from_value::<Package>(json).is_err());
}

#[test]
fn enabling_requires_declarations_for_the_actual_component_capabilities() {
    let mut package = package();
    package.manifest.execution = model::DECLARATIVE.into();
    package.payload = serde_json::json!({"format":nucleus::canvas::Document::FORMAT,"name":"Terminal","component":{"source":"native","kind":"terminal","settings":{},"bindings":[]}}).to_string();
    package.manifest.permissions = model::required_permissions(&package.payload).unwrap();
    package.digest = package.content_digest().unwrap();
    assert!(
        package
            .manifest
            .permissions
            .contains(&"terminal:execute".to_string())
    );
    assert!(
        package
            .manifest
            .permissions
            .contains(&"native:terminal".to_string())
    );
    package.validate_execution().unwrap();
    package
        .manifest
        .permissions
        .retain(|permission| permission != "terminal:execute");
    package.digest = package.content_digest().unwrap();
    package.validate().unwrap();
    assert!(package.validate_execution().is_err());
    package.manifest.execution = model::DESKTOP_LAYOUT.into();
    package.digest = package.content_digest().unwrap();
    assert!(package.validate_execution().is_err());
}
