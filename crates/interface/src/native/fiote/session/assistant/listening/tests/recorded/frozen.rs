use super::*;
use std::{collections::BTreeMap, path::Path};

fn samples(root: &Path, name: &str, hashes: &BTreeMap<String, String>) -> Vec<f32> {
    assert!(
        name.chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-')
    );
    let bytes = std::fs::read(root.join(format!("{name}.wav"))).unwrap();
    assert_eq!(
        nucleus::message::digest(&bytes),
        hashes[&format!("{name}.wav")]
    );
    cell::fiote_communication::speech::decode_audio(&STANDARD.encode(bytes)).unwrap()
}

#[test]
fn frozen_keyword_requests_and_quotes_use_the_native_gate_without_spurious_delivery() {
    let root = std::env::var_os("LINCE_WAKE_ACCEPTANCE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fiote/tests/fixtures/wake-acceptance")
        });
    let protocol: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("protocol.json")).unwrap()).unwrap();
    let hashes = std::fs::read_to_string(root.join("audio.sha256"))
        .unwrap()
        .lines()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next().unwrap().to_owned();
            let file = Path::new(parts.next().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            (file, hash)
        })
        .collect::<BTreeMap<_, _>>();
    let reference: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/fixtures/english-conversation-reference.json"
    )))
    .unwrap();
    let heart = ["wake-statement", "wake-question", "wake-exclamation"].map(|name| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fiote/tests/fixtures/conversation")
            .join(format!("{name}.wav"));
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            nucleus::message::digest(&bytes),
            reference["cases"][name]["sha256"].as_str().unwrap()
        );
        cell::fiote_communication::speech::decode_audio(&STANDARD.encode(bytes)).unwrap()
    });
    let michael = ["m-e01", "m-e02", "m-e03"].map(|name| samples(&root, name, &hashes));
    let profiles = BTreeMap::from([
        (
            "heart",
            cell::fiote_communication::speech::wake::Profile::enroll(&heart).unwrap(),
        ),
        (
            "michael",
            cell::fiote_communication::speech::wake::Profile::enroll(&michael).unwrap(),
        ),
    ]);
    let mut accepted = BTreeMap::from([("heart", 0usize), ("michael", 0usize)]);
    for case in protocol["cases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        if kind == "reference" {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let owner = case["profile"].as_str().unwrap();
        let mut input = vec![0.0; 32592];
        input.extend(samples(&root, name, &hashes));
        input.extend(vec![0.0; 32000]);
        let mut gate = acoustic::Gate::new(profiles[owner].clone(), 7).unwrap();
        let events = input
            .chunks(1600)
            .flat_map(|part| gate.push(part).unwrap())
            .collect::<Vec<_>>();
        let deliveries = events
            .iter()
            .filter(|event| matches!(event, Event::WokenSegment { .. } | Event::Segment { .. }))
            .count();
        println!("native_case={name} kind={kind} deliveries={deliveries}");
        if kind == "request" {
            assert!(deliveries <= 1, "Duplicate request for {name}");
            if deliveries == 1 {
                *accepted.get_mut(owner).unwrap() += 1;
            }
        } else {
            assert_eq!(
                deliveries, 0,
                "Non-request must never reach ASR/provider: {name}"
            );
        }
    }
    assert!(
        accepted["heart"] >= 9,
        "Primary request delivery: {accepted:?}"
    );
    assert_eq!(
        accepted["michael"], 3,
        "Second voice delivery: {accepted:?}"
    );
}
