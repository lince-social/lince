use base64::{Engine as _, engine::general_purpose::STANDARD};
use fiote::{
    communication::models::digest,
    speech::{
        decode_audio,
        wake::{Detector, Profile},
    },
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn audio(root: &Path, name: &str, hashes: &BTreeMap<String, String>) -> Vec<f32> {
    assert!(
        name.chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    );
    let bytes = std::fs::read(root.join(format!("{name}.wav"))).unwrap();
    assert_eq!(digest(&bytes), hashes[&format!("{name}.wav")]);
    decode_audio(&STANDARD.encode(bytes)).unwrap()
}

#[test]
fn two_voice_keyword_regressions_without_asr_or_cloud() {
    let root = std::env::var_os("LINCE_WAKE_ACCEPTANCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wake-acceptance")
        });
    let protocol: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("protocol.json")).unwrap()).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let hashes = std::fs::read_to_string(root.join("audio.sha256"))
        .unwrap()
        .lines()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next().unwrap().to_owned();
            let name = Path::new(parts.next().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            (name, hash)
        })
        .collect::<BTreeMap<_, _>>();
    let references: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/english-conversation-reference.json")).unwrap();
    let heart = ["wake-statement", "wake-question", "wake-exclamation"].map(|name| {
        let bytes = std::fs::read(
            source
                .join("tests/fixtures/conversation")
                .join(format!("{name}.wav")),
        )
        .unwrap();
        assert_eq!(
            digest(&bytes),
            references["cases"][name]["sha256"].as_str().unwrap()
        );
        decode_audio(&STANDARD.encode(bytes)).unwrap()
    });
    let michael = ["m-e01", "m-e02", "m-e03"].map(|name| audio(&root, name, &hashes));
    let profiles = BTreeMap::from([
        ("heart", Profile::enroll(&heart).unwrap()),
        ("michael", Profile::enroll(&michael).unwrap()),
    ]);
    for profile in profiles.values() {
        println!(
            "profile_version={} threshold={}",
            profile.version, profile.threshold
        );
    }
    for case in protocol["cases"].as_array().unwrap() {
        let _ = audio(&root, case["name"].as_str().unwrap(), &hashes);
    }
    let mut accepted = BTreeMap::from([("heart", 0usize), ("michael", 0usize)]);
    let mut failures = Vec::new();
    for case in protocol["cases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        if kind == "reference" {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let owner = case["profile"].as_str().unwrap();
        let recording = audio(&root, name, &hashes);
        let mut input = vec![0.0; 32592];
        input.extend_from_slice(&recording);
        input.extend(vec![0.0; 32_000]);
        let mut previous = None;
        let mut matched = false;
        for chunk_size in [97, 1600] {
            let mut detector = Detector::new(profiles[owner].clone()).unwrap();
            let detections = input
                .chunks(chunk_size)
                .filter_map(|part| detector.push(part).unwrap())
                .collect::<Vec<_>>();
            let endpoints = detections
                .iter()
                .map(|detection| detection.end_sample)
                .collect::<Vec<_>>();
            if let Some(previous) = &previous {
                assert_eq!(&endpoints, previous, "Chunk-dependent endpoint for {name}");
            }
            previous = Some(endpoints);
            if detections.len() > 1 || (kind == "negative" && !detections.is_empty()) {
                failures.push(format!("{name}: unexpected activation(s)"));
            }
            if kind == "request" && detections.len() == 1 {
                let end = detections[0].end_sample;
                assert!(
                    (32592..=32592 + recording.len() as u64).contains(&end),
                    "Endpoint outside recording for {name}"
                );
                matched = true;
            }
            println!(
                "case={name} profile={owner} kind={kind} chunk={chunk_size} detections={detections:?}"
            );
        }
        if kind == "request" && matched {
            *accepted.get_mut(owner).unwrap() += 1;
        }
    }
    println!(
        "accepted={accepted:?} failures={failures:?}; quote-only request suppression is checked by the native gate test"
    );
    assert!(
        failures.is_empty(),
        "Frozen negative/replay failures: {failures:?}"
    );
    assert!(
        accepted["heart"] >= 9,
        "Fewer than nine of ten primary requests detected"
    );
    assert_eq!(accepted["michael"], 3, "Cross-voice acceptance failed");
}
