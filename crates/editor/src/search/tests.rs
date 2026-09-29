use super::*;

#[test]
fn matches_cross_chunks_and_wrap_in_both_directions() {
    let text = format!("{}猫abc\nDEf猫 end", "x".repeat(16_382));
    let rope = Rope::from_str(&text);
    let cancel = AtomicBool::new(false);
    let pattern = Pattern::new("猫abc\nDEf猫", Options::default()).unwrap();
    assert_eq!(
        pattern.find(&rope, 0, false, &cancel).unwrap(),
        Some(16_382..16_391)
    );
    assert_eq!(
        pattern
            .find(&rope, rope.len_chars(), false, &cancel)
            .unwrap(),
        Some(16_382..16_391)
    );
    assert_eq!(
        pattern.find(&rope, 0, true, &cancel).unwrap(),
        Some(16_382..16_391)
    );
}

#[test]
fn unicode_case_and_word_boundaries_keep_context_at_chunk_edges() {
    let rope = Rope::from_str(&format!("{}café CAFÉ caféine xCAT CAT", " ".repeat(16_380)));
    let options = Options {
        case_sensitive: false,
        whole_word: true,
    };
    let cancel = AtomicBool::new(false);
    let pattern = Pattern::new("CAFÉ", options).unwrap();
    assert_eq!(
        pattern.all(&rope, &cancel).unwrap(),
        vec![16_380..16_384, 16_385..16_389]
    );
    let pattern = Pattern::new("CAT", options).unwrap();
    assert_eq!(
        pattern.find(&rope, 16_400, false, &cancel).unwrap(),
        Some(16_403..16_406)
    );
}

#[test]
fn long_matches_are_not_repeated_at_chunk_boundaries() {
    let rope = Rope::from_str(&"a".repeat(40_000));
    let pattern = Pattern::new(&"a".repeat(3000), Options::default()).unwrap();
    let all = pattern.all(&rope, &AtomicBool::new(false)).unwrap();
    assert_eq!(all.len(), 13);
    for (i, range) in all.iter().enumerate() {
        assert_eq!(*range, i * 3000..(i + 1) * 3000);
    }
    assert!(pattern.all(&rope, &AtomicBool::new(true)).is_err());
}
