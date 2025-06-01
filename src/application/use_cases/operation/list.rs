pub fn operation_tables() -> Vec<Vec<&'static str>> {
    vec![
        vec!["0", "Configuration"],
        vec!["1", "collection"],
        vec!["2", "View"],
        vec!["3", "collection_View"],
        vec!["4", "Record"],
        vec!["5", "Karma_Condition"],
        vec!["6", "Karma_Consequence"],
        vec!["7", "Karma"],
        vec!["8", "Command"],
        vec!["9", "Frequency"],
        vec!["10", "Sum"],
        vec!["11", "History"],
        vec!["12", "DNA"],
        vec!["13", "Transfer"],
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
