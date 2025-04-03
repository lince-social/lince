use regex::Regex;
use std::io::Error;

pub async fn execute_operation(operation: String) -> Result<String, Error> {
    let re = Regex::new(r"\d+").unwrap();

    let table = if let Some(matched) = re.find(&operation) {
        match matched.as_str() {
            "0" => "configuration",
            "1" => "view",
            "2" => "configuration_view",
            "3" => "record",
            "4" => "karma_condition",
            "5" => "karma_consequence",
            "6" => "karma",
            "7" => "command",
            "8" => "frequency",
            "9" => "sum",
            "10" => "history",
            "11" => "dna",
            "12" => "transfer",
            _ => "record",
        }
    } else {
        "record"
    };

    // let re = Regex::new(r"[a-z]+").unwrap();
    // if let Some(matched) = re.find(&operation) {
    //     match matched.as_str() {
    //         "c" => tui,
    //     }
    // }

    Ok(table.to_string())
}
