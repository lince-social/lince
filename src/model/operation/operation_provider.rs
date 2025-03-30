use regex::Regex;
use std::io::Error;

pub async fn execute_operation(operation: String) -> Result<String, Error> {
    let re = Regex::new(r"\d+").unwrap();
    let mut table = "";

    if let Some(matched) = re.find(&operation) {
        match matched.as_str() {
            "0" => table = "configuration",
            "1" => table = "view",
            "2" => table = "configuration_view",
            "3" => table = "record",
            "4" => table = "karma_condition",
            "5" => table = "karma_consequence",
            "6" => table = "karma",
            "7" => table = "command",
            "8" => table = "frequency",
            "9" => table = "sum",
            "10" => table = "history",
            "11" => table = "dna",
            "12" => table = "transfer",
            _ => table = "record",
        }
    } else {
        table = "record";
    }

    // let re = Regex::new(r"[a-z]+").unwrap();
    // if let Some(matched) = re.find(&operation) {
    //     match matched.as_str() {
    //         "c" => tui,
    //     }
    // }
    println!("{table}");

    Ok(table.to_string())
}
