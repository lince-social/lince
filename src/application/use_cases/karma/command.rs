use crate::infrastructure::{
    cross_cutting::InjectedServices,
    utils::log::{LogEntry, log},
};
use std::io::ErrorKind;
use tokio::process::Command as TokioCommand;

pub async fn use_case_karma_execute_command(services: InjectedServices, id: u32) -> Option<i64> {
    let res = services.providers.command.get_by_id(id).await;
    match res {
        Err(e) => {
            log(LogEntry::Error(
                e.kind(),
                format!("Error when getting command with id: {}. Error: {}", id, e),
            ));
            None
        }
        Ok(opt) => match opt {
            None => None,
            Some(command) => service_karma_execute_command(command.command).await,
        },
    }
}

pub async fn service_karma_execute_command(command: String) -> Option<i64> {
    let output = TokioCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await;

    match output {
        Ok(output) => {
            if !output.status.success() {
                log(LogEntry::Error(
                    ErrorKind::Other,
                    format!(
                        "Command '{}' failed with status: {}. Stderr: {}",
                        command,
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
                return None;
            }
            Some(0)
        }
        Err(e) => {
            log(LogEntry::Error(
                e.kind(),
                format!("Failed to execute command '{}': {}", command, e),
            ));
            None
        }
    }
    // let status = match TokioCommand::new("sh")
    //     .arg("-c")
    //     .arg(&command)
    //     .stdin(Stdio::inherit())
    //     .stdout(Stdio::inherit())
    //     .stderr(Stdio::inherit())
    //     .status()
    //     .await
    // {
    //     Ok(s) => s,
    //     Err(e) => {
    //         log(LogEntry::Error(
    //             e.kind(),
    //             format!("Error when running command '{}': {}", command, e),
    //         ));
    //         return None;
    //     }
    // };

    // if !status.success() {
    //     log(LogEntry::Error(
    //         ErrorKind::Other,
    //         format!("Command '{}' exited with non-zero status", command),
    //     ));
    //     return None;
    // }

    // Some(0)
}
