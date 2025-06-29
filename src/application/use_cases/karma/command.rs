use crate::infrastructure::{
    cross_cutting::InjectedServices,
    utils::log::{LogEntry, log},
};
use std::{io::ErrorKind, process::Stdio};
use tokio::process::Command as TokioCommand;

pub async fn use_case_karma_execute_command(services: InjectedServices, id: u32) -> Option<i64> {
    let res = services.providers.command.get_by_id(id).await;
    dbg!(&res);
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
    log(LogEntry::Message(format!("Running command: {command}")));

    let status = match TokioCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
    {
        Ok(s) => s,
        Err(e) => {
            log(LogEntry::Error(
                e.kind(),
                format!("Error when running command '{}': {}", command, e),
            ));
            return None;
        }
    };

    if !status.success() {
        log(LogEntry::Error(
            ErrorKind::Other,
            format!("Command '{}' exited with non-zero status", command),
        ));
        return None;
    }

    Some(0)
}
// pub async fn service_karma_execute_command(command: String) -> Option<i64> {
//     log(LogEntry::Message(format!("Running command: {command}")));

//     let status = TokioCommand::new("sh")
//         .arg("-c")
//         .arg(command)
//         .stdin(Stdio::inherit())
//         .stdout(Stdio::inherit())
//         .stderr(Stdio::inherit())
//         .status()
//         .await;

//     if let Err(e) = status.as_ref() {
//         log(LogEntry::Error(
//             e.kind(),
//             format!("Error when running command. Error: {}", e.to_string()),
//         ));
//     }

//     if !status.unwrap().success() {
//         return None;
//     }

//     Some(0)
// }

// pub async fn service_karma_execute_command(command: String) -> Option<i64> {
//     log(LogEntry::Message(format!("Running command: {command}")));

//     let status = TokioCommand::new("sh")
//         .arg("-c")
//         .arg(command)
//         .stdin(Stdio::inherit())
//         .stdout(Stdio::inherit())
//         .stderr(Stdio::inherit())
//         .status()
//         .await
//         .ok()?;

//     if !status.success() {
//         return None;
//     }

//     Some(0)
// }
