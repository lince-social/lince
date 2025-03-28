use std::io::{self, Error, ErrorKind, Read};

pub async fn execute_operation(operation: String) -> Result<String, Error> {
    let int = operation.parse::<u32>();
    let wait;
    if int.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Error when parsing table, incorrect integer passed",
        ));
        io::stdin().read_line(wait);
    }

    let int = int.unwrap();
    println!("{int}");

    Ok(operation)
    // Ok("All gucci".to_string())
}
