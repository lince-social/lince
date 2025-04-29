use std::{io, process::Command};

use crate::application::use_cases::operation::{
    execute::use_case_operation_execute,
    list_operations::{operation_actions, operation_tables},
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

        use_case_operation_execute(input).await;
    }
}
