use std::{io, process::Command};

use crate::model::operation::{
    operation_information::{operation_actions, operation_tables},
    operation_provider::execute_operation,
};

pub async fn run_tui_mode() {
    loop {
        let _ = Command::new("clear").status();
        println!("Operation Tables:");
        for table in operation_tables() {
            println!("{} | {}", table[0], table[1])
        }

        println!(
            r"
Operation Actions:"
        );
        for action in operation_actions() {
            println!("{} | {}", action[0], action[1])
        }

        println!(
            r"
Your choice: "
        );
        let mut input = String::new();

        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");

        let input = input.trim().to_string();

        if input == "exit" {
            break;
        }

        if let Err(e) = execute_operation(input).await {
            eprintln!("Error when executing operation: {:#?}", e);
            println!("(Press enter to continue)");
            let _ = io::stdin().read_line(&mut String::new());
        }
    }
}
