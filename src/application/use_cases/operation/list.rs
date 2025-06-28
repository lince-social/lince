pub fn operation_tables() -> Vec<(&'static str, &'static str)> {
    vec![
        ("0", "Configuration"),
        ("1", "Collection"),
        ("2", "View"),
        ("3", "collection_View"),
        ("4", "Record"),
        ("5", "Karma_Condition"),
        ("6", "Karma_Consequence"),
        ("7", "Karma"),
        ("8", "Command"),
        ("9", "Frequency"),
        ("10", "Sum"),
        ("11", "History"),
        ("12", "DNA"),
        ("13", "Transfer"),
    ]
}

pub fn operation_actions() -> Vec<(&'static str, &'static str)> {
    vec![
        ("c", "Create"),
        ("q", "SQL Query"),
        ("k", "Karma"),
        ("s", "Shell Command"),
        ("a", "Activate Configuration"),
        // ("t", "Transfer"),
        // ("r", "Read"),
        // ("u", "Update"),
        // ("d", "Delete"),
    ]
}
