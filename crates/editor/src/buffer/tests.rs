use super::*;

#[test]
fn an_intermediate_truncated_disk_write_cannot_erase_unrelated_local_edits() {
    let mut buffer = Buffer::new("a\nb\nc\n").unwrap();
    buffer
        .edit(Edit {
            range: 2..3,
            text: "B".into(),
        })
        .unwrap();
    buffer.reconcile("a\n").unwrap();
    assert!(buffer.conflict().is_some());
    assert_eq!(buffer.text(), "a\nB\nc\n");
    buffer.reconcile("external\nb\nc\n").unwrap();
    assert!(buffer.conflict().is_none());
    assert_eq!(buffer.text(), "external\nB\nc\n");
    buffer.undo(false).unwrap();
    assert_eq!(buffer.text(), "external\nb\nc\n");
}

#[test]
fn a_bounded_window_keeps_complete_lines_editable() {
    let line = format!("{}\n", "x".repeat(1000));
    let buffer = Buffer::new(&line.repeat(100)).unwrap();
    let window = buffer.window(0, 96);
    assert!(!window.clipped);
    assert!(window.text.ends_with('\n'));
    assert!(window.text.len() <= MAX_WINDOW_BYTES);
    assert!(window.text.lines().count() < 96);
}

#[test]
fn saving_invalidates_an_older_disk_comparison_even_without_further_typing() {
    let mut buffer = Buffer::new("original").unwrap();
    buffer
        .edit(Edit {
            range: 0..8,
            text: "local".into(),
        })
        .unwrap();
    let stale = buffer.reconciliation().compute("external".into()).unwrap();
    let saved = buffer.prepare_save().unwrap();
    buffer.saved(saved).unwrap();
    assert!(!buffer.accept(stale).unwrap());
    assert_eq!(buffer.text(), "local");
}

#[test]
fn stale_background_comparisons_are_retried_without_losing_typing() {
    let mut buffer = Buffer::new("alpha\nbeta\n").unwrap();
    let comparison = buffer
        .reconciliation()
        .compute("alpha\ndisk\n".into())
        .unwrap();
    buffer
        .edit(Edit {
            range: 0..5,
            text: "typed".into(),
        })
        .unwrap();
    assert!(!buffer.accept(comparison).unwrap());
    assert_eq!(buffer.text(), "typed\nbeta\n");
    buffer.reconcile("alpha\ndisk\n").unwrap();
    assert_eq!(buffer.text(), "typed\ndisk\n");
}

#[test]
fn repeated_disk_edits_and_a_later_save_keep_the_common_history() {
    let mut buffer = Buffer::new("a\nb\nc\n").unwrap();
    buffer
        .edit(Edit {
            range: 0..1,
            text: "mine".into(),
        })
        .unwrap();
    for value in ["a\nb\nC\n", "a\nb\nCC\n", "a\nb\nc\n"] {
        buffer.reconcile(value).unwrap();
        assert!(buffer.conflict().is_none());
        assert!(buffer.text().starts_with("mine\n"));
    }
    let saved = buffer.prepare_save().unwrap();
    buffer.saved(saved).unwrap();
    buffer.reconcile("mine\nexternal\nc\n").unwrap();
    assert_eq!(buffer.text(), "mine\nexternal\nc\n");
    assert!(!buffer.is_dirty());
}

#[test]
fn independent_disk_edits_merge_and_undo_keeps_disk_changes() {
    let mut buffer = Buffer::new("one\ntwo\nthree\n").unwrap();
    buffer
        .edit(Edit {
            range: 0..3,
            text: "local".into(),
        })
        .unwrap();
    buffer.reconcile("one\ntwo\nexternal\n").unwrap();
    assert_eq!(buffer.text(), "local\ntwo\nexternal\n");
    assert!(buffer.conflict().is_none());
    assert!(buffer.is_dirty());
    buffer.undo(false).unwrap();
    assert_eq!(buffer.text(), "one\ntwo\nexternal\n");
}

#[test]
fn overlap_waits_for_a_choice_and_identical_edits_do_not_duplicate() {
    let mut buffer = Buffer::new("hello\n").unwrap();
    buffer
        .edit(Edit {
            range: 0..5,
            text: "local".into(),
        })
        .unwrap();
    buffer.reconcile("disk\n").unwrap();
    assert_eq!(buffer.text(), "local\n");
    assert!(buffer.prepare_save().is_err());
    buffer.resolve(Resolution::Local).unwrap();
    assert_eq!(buffer.text(), "local\n");
    let saved = buffer.prepare_save().unwrap();
    buffer.saved(saved).unwrap();
    buffer
        .edit(Edit {
            range: 0..5,
            text: "same".into(),
        })
        .unwrap();
    buffer.reconcile("same\n").unwrap();
    assert_eq!(buffer.text(), "same\n");
    assert!(!buffer.is_dirty());
    buffer.reconcile("next\n").unwrap();
    assert_eq!(buffer.text(), "next\n");
}

#[test]
fn unicode_crlf_bom_and_window_boundaries_are_preserved() {
    let mut buffer = Buffer::new("猫🐈\r\ncafé\r\n").unwrap();
    buffer
        .edit(Edit {
            range: 1..2,
            text: "é".into(),
        })
        .unwrap();
    assert_eq!(buffer.text(), "猫é\r\ncafé\r\n");
    let window = buffer.window(1, 1);
    assert_eq!(window.text, "café\r\n");
    assert_eq!(window.start, 4);
    buffer.reconcile("猫🐈\r\ncafé!\r\n").unwrap();
    assert_eq!(buffer.text(), "猫é\r\ncafé!\r\n");
    assert_eq!(Edit::between("猫🐈", "猫é", 4).unwrap().range, 5..6);
}

#[test]
fn edits_during_save_remain_dirty() {
    let mut buffer = Buffer::new("abc").unwrap();
    buffer
        .edit(Edit {
            range: 3..3,
            text: "d".into(),
        })
        .unwrap();
    let point = buffer.prepare_save().unwrap();
    buffer
        .edit(Edit {
            range: 4..4,
            text: "e".into(),
        })
        .unwrap();
    buffer.saved(point).unwrap();
    assert!(buffer.is_dirty());
    assert_eq!(buffer.observed(), "abcd");
    assert_eq!(buffer.text(), "abcde");
    buffer.reconcile("Abcd").unwrap();
    assert_eq!(buffer.text(), "Abcde");
    assert!(buffer.conflict().is_none());
}

#[test]
fn deferred_encoding_contains_only_the_requested_save_version() {
    let mut buffer = Buffer::new("one\ntwo\nthree\n").unwrap();
    buffer
        .edit(Edit {
            range: 0..3,
            text: "saved".into(),
        })
        .unwrap();
    let point = buffer.prepare_save().unwrap();
    buffer
        .edit(Edit {
            range: 6..9,
            text: "later".into(),
        })
        .unwrap();
    let point = std::thread::spawn(move || point.encode()).join().unwrap();
    let text = point.text.to_string().into();
    buffer.saved_with_text(point, text).unwrap();
    buffer.reconcile("saved\ntwo\ndisk\n").unwrap();
    assert_eq!(buffer.text(), "saved\nlater\ndisk\n");
    assert!(buffer.is_dirty());
    assert!(buffer.conflict().is_none());
}

#[test]
fn large_file_save_snapshots_preserve_newer_edits() {
    let value = "text 猫\n".repeat(150_000);
    let mut buffer = Buffer::new(&value).unwrap();
    buffer
        .edit_batch(&[Edit {
            range: 0..0,
            text: "first\n".into(),
        }])
        .unwrap();
    let point = buffer.prepare_save().unwrap();
    let saved_text: Arc<str> = point.text.to_string().into();
    buffer
        .edit_batch(&[Edit {
            range: 0..0,
            text: "later\n".into(),
        }])
        .unwrap();
    buffer.saved_with_text(point.encode(), saved_text).unwrap();
    assert!(buffer.is_dirty());
    assert!(buffer.observed().starts_with("first\ntext"));
    buffer.undo(false).unwrap();
    assert_eq!(buffer.text(), format!("first\n{value}"));
}

#[test]
fn large_files_have_bounded_windows_and_incremental_unicode_edits() {
    let value = "some plain text 猫\n".repeat(300_000);
    let mut buffer = Buffer::new(&value).unwrap();
    let window = buffer.window(200_000, 80);
    assert!(window.text.len() < MAX_WINDOW_BYTES);
    assert_eq!(window.total_lines, 300_001);
    buffer
        .edit(Edit {
            range: window.start..window.start,
            text: "!".into(),
        })
        .unwrap();
    assert!(buffer.window(200_000, 80).text.starts_with('!'));
    let long = Buffer::new(&"猫".repeat(30_000)).unwrap().window(0, 80);
    assert!(long.clipped);
    assert!(long.text.len() <= MAX_WINDOW_BYTES);
}
