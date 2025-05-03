use crate::application::providers::command::get::provider_karma_get_command_by_id;
use std::process::Stdio;
use tokio::process::Command;

pub fn use_case_karma_execute_command(id: u32) -> Option<i64> {
    futures::executor::block_on(async {
        let command = provider_karma_get_command_by_id(id).await;
        if command.is_err() {
            return None;
        }
        let command = command.unwrap();

        service_karma_execute_command(command).await
    })
}

async fn service_karma_execute_command(command: String) -> Option<i64> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.trim().parse::<i64>().ok()
}
