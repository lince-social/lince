pub fn operation_tables() -> Vec<Vec<&'static str>> {
    vec![
        vec!["0", "Configuration"],
        vec!["1", "View"],
        vec!["2", "Configuration_View"],
        vec!["3", "Record"],
        vec!["4", "Karma_Condition"],
        vec!["5", "Karma_Consequence"],
        vec!["6", "Karma"],
        vec!["7", "Command"],
        vec!["8", "Frequency"],
        vec!["9", "Sum"],
        vec!["10", "History"],
        vec!["11", "DNA"],
        vec!["12", "Transfer"],
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
