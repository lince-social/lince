use super::*;

fn composer(private: bool) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let binding = RecordBinding {
        area: world.spawn_empty().id(),
        uid: "record".into(),
        source: crate::protein_area::Source::Local,
    };
    let owner = world.spawn(Node::default()).id();
    let input = world
        .spawn(crate::sand::text_editor(
            "Inspect these files",
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    let status = crate::edit_mode::label(&mut world, owner, "", 12.0);
    draft(
        &mut world,
        owner,
        input,
        status,
        "attachment-test",
        &binding,
        private,
    );
    (world, owner)
}

#[test]
fn all_conversation_composers_keep_files_and_dictation_without_private_record_controls() {
    for private in [false, true] {
        let (mut world, owner) = composer(private);
        let labels: Vec<_> = world
            .query::<&Text>()
            .iter(&world)
            .map(|text| text.0.clone())
            .collect();
        for label in [
            "Attach files",
            "Paste image",
            "Dictate",
            "Stop and transcribe",
            "Cancel dictation",
        ] {
            assert!(labels.iter().any(|text| text == label), "{label}");
        }
        assert_eq!(
            world.get::<Draft>(owner).unwrap().reference.is_none(),
            private
        );
    }
}

#[test]
fn six_file_formats_keep_bytes_remove_selection_and_survive_failed_sends() {
    let directory = tempfile::tempdir().unwrap();
    let (mut world, owner) = composer(true);
    let fixtures = [
        ("note.txt", "text/plain"),
        ("table.csv", "text/csv"),
        ("document.pdf", "application/pdf"),
        ("photo.png", "image/png"),
        ("audio.wav", "audio/wav"),
        ("video.mp4", "video/mp4"),
    ];
    let mut parts = Vec::new();
    for (index, (name, mime_type)) in fixtures.into_iter().enumerate() {
        let bytes = vec![index as u8; 32 + index];
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let part = read_file(path, false).unwrap();
        assert!(
            matches!(&part, MessagePart::Attachment { name: held, mime_type: mime, data }
            if held == name && mime == mime_type && nucleus::message::decode(data).unwrap() == bytes)
        );
        parts.push(part);
    }
    append(&mut world, owner, parts.clone());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    Remove(1).apply(&mut world, owner);
    parts.remove(1);
    world.get_mut::<Draft>(owner).unwrap().locked = true;
    sent(&mut world, owner, false);
    assert_eq!(contents(&world, owner).unwrap(), parts);
    append(&mut world, owner, Vec::new());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    let oversized = directory.path().join("oversized.bin");
    std::fs::write(&oversized, vec![0; nucleus::message::MAX_CONTENT_BYTES + 1]).unwrap();
    assert!(read_file(oversized, false).is_err());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    sent(&mut world, owner, true);
    assert!(contents(&world, owner).unwrap().is_empty());
}
