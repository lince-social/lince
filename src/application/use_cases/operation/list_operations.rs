pub fn operation_tables() -> Vec<Vec<&'static str>> {
    vec![
        vec!["0", "Configuration"],
        vec!["1", "View"],
        vec!["2", "Configuration_View"],
        vec!["3", "Record"],
    ]
}

pub fn operation_actions() -> Vec<Vec<&'static str>> {
    vec![
        vec!["c", "Create"],
        vec!["r", "Read"],
        vec!["u", "Update"],
        vec!["d", "Delete"],
    ]
}
