fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(check)
        .unwrap()
        .join()
        .unwrap();
}

fn check() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let cases: Vec<_> = lince_desktop::laboratory::catalogue()
        .into_iter()
        .filter(|case| {
            case.name
                .ends_with("relation_castle_area_tab_opens_and_edits_spawn_configuration")
                || case.name.contains("protein_area::record_layout::tests::")
        })
        .collect();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let result = case.execute(&runtime);
        assert!(
            result.error.is_none(),
            "{}: {:?}",
            result.name,
            result.error
        );
        println!("PASS {} ({:.1} ms)", result.name, result.milliseconds);
    }
}
