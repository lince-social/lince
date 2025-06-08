use crate::infrastructure::cross_cutting::InjectedServices;
use std::process::Stdio;
use tokio::process::Command as TokioCommand;

pub async fn use_case_karma_execute_command(services: InjectedServices, id: u32) -> Option<i64> {
    let command = services.providers.command.get_by_id(id).await.ok()?;

    let command = command.unwrap();

    service_karma_execute_command(command.command).await
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

pub async fn use_case_karma_command(services: InjectedServices, id: String) -> Command {
    let command = services.providers.command.get_by_id(id).await;
    command
}

pub async fn use_case_karma_command_name(services: InjectedServices, id: String) -> String {
    match services.providers.command.get_by_id(id).await {
        Some(command) => command.name,
        None => String::new(),
    }
}
