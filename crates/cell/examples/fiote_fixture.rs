use cell::{Cell, CellOptions, FioteSecret, fiote::Host};
use fiote::config::Request;
use std::{io::Error, path::PathBuf};
use transport::fiote::Service;

mod native_connection {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fiote/tests/support/native_connection.rs"
    ));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let directory = PathBuf::from(
        arguments
            .next()
            .ok_or_else(|| Error::other("Choose a new disposable directory"))?,
    );
    if arguments.next().is_some() || directory.exists() {
        return Err(Error::other(
            "Choose a new disposable directory; existing data is never replaced",
        )
        .into());
    }
    let (mut settings, credential) = native_connection::existing_connection().await;
    std::fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    }
    settings.directory = directory.join("workspace");
    std::fs::create_dir(&settings.directory)?;
    settings.enabled = true;
    let cell = Cell::open_mobile(CellOptions {
        data_dir: Some(directory.clone()),
        peer_port: Some(0),
        ..Default::default()
    })
    .await?;
    let host = Host::open(cell.runtime().engine.clone(), directory.join("fiote"))
        .await
        .map_err(Error::other)?;
    let selected = match host.intake_selection().await.map_err(Error::other)? {
        Some(intake) => intake.record,
        None => {
            host.handle(Request::Create {
                head: "Acceptance Fiote".into(),
            })
            .await
            .map_err(Error::other)?
            .record
        }
    };
    let record = host
        .intake_selection()
        .await
        .map_err(Error::other)?
        .map_or(selected, |intake| intake.record);
    let password = FioteSecret(nucleus::new_uid("acceptance-vault"));
    let password_file = directory.join("vault-password");
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&password_file)?
            .write_all(password.0.as_bytes())?;
    }
    host.handle(Request::Configure {
        record: record.clone(),
        settings,
        api_key: Some(credential),
        password: Some(password),
    })
    .await
    .map_err(Error::other)?;
    host.stop_all().await;
    cell.shutdown().await;
    let report = serde_json::json!({"record":record,"directory":directory,"password_file":password_file,"credentials":"encrypted access token only; refresh token removed","working_directory":directory.join("workspace")});
    serde_json::to_writer_pretty(
        std::fs::File::create(directory.join("fixture.json"))?,
        &report,
    )?;
    println!("{report}");
    Ok(())
}
