use crate::application::providers::command::get::provider_karma_get_command_by_id;
use std::process::Stdio;
use tokio::process::Command as TokioCommand;

pub fn use_case_karma_execute_command(id: u32) -> Option<i64> {
    futures::executor::block_on(async {
        let command = provider_karma_get_command_by_id(id).await;
        if command.is_err() {
            return None;
        }
        let command = command.unwrap();

        service_karma_execute_command(command.command).await
    })
}
pub async fn service_karma_execute_command(command: String) -> Option<i64> {
    let status = TokioCommand::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .ok()?;

    if !status.success() {
        return None;
    }

    Some(0)
}

pub async fn use_case_command_get_name(id: u32) -> Option<String> {
    match provider_karma_get_command_by_id(id).await {
        Err(_) => None,
        Ok(value) => Some(value.name),
    }
}
