use base64::{Engine, prelude::BASE64_STANDARD};
use lince_sandbox::physical_workspace::adapter::{
    Command, Descriptor, PROTOCOL_VERSION, Packet, Reply,
};
use nucleus::physical_workspace::{Evidence, Interaction, Phase};
use std::io::Read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn bounded_file(path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Input file exceeds the allowed size.".into());
    }
    Ok(bytes)
}

async fn exchange(stream: &mut tokio::net::TcpStream, packet: Packet) -> Result<Reply, String> {
    let bytes = serde_json::to_vec(&packet).map_err(|e| e.to_string())?;
    if bytes.len() > 1_048_576 {
        return Err("Packet exceeds 1 MiB.".into());
    }
    stream
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(&bytes).await.map_err(|e| e.to_string())?;
    let size = stream.read_u32().await.map_err(|e| e.to_string())?;
    if size > 16_384 {
        return Err("Unexpected reply size.".into());
    }
    let mut reply = vec![0; size as usize];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&reply).map_err(|e| e.to_string())
}

async fn run(arguments: Vec<String>) -> Result<(), String> {
    let file = arguments.get(1).ok_or("Usage: physical_adapter_client PAIRING_JSON [clock|observe|drag|frame] [surface] [JPEG_PATH]")?;
    let descriptor: Descriptor =
        serde_json::from_slice(&bounded_file(file, 8192)?).map_err(|e| e.to_string())?;
    let address: std::net::SocketAddr = descriptor
        .address
        .parse()
        .map_err(|_| "Invalid endpoint address.")?;
    if !address.ip().is_loopback() || descriptor.version != PROTOCOL_VERSION {
        return Err("Use a supported local adapter descriptor.".into());
    }
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .map_err(|e| e.to_string())?;
    let mut sequence = 1;
    let mut reply = exchange(
        &mut stream,
        Packet {
            version: PROTOCOL_VERSION,
            token: descriptor.token.clone(),
            sequence,
            command: Command::Clock,
        },
    )
    .await?;
    let mode = arguments.get(2).map(String::as_str).unwrap_or("clock");
    let surface = arguments
        .get(3)
        .map(String::as_str)
        .unwrap_or(&descriptor.surface);
    if surface != descriptor.surface {
        return Err("Enable and copy pairing details for the intended surface first.".into());
    }
    let steps = match mode {
        "clock" => 0,
        "observe" | "frame" => 1,
        "drag" => 2,
        _ => return Err("Choose clock, observe, drag or frame.".into()),
    };
    for step in 0..steps {
        sequence += 1;
        let evidence_sequence = reply
            .now_ms
            .checked_mul(1024)
            .and_then(|now| now.checked_add(sequence))
            .ok_or("Clock sequence overflow.")?;
        let id = nucleus::new_uid("observation");
        let evidence = Evidence {
            id: id.clone(),
            device: descriptor.device.clone(),
            session: descriptor.session.clone(),
            sequence: evidence_sequence,
            captured_ms: reply.now_ms,
            surface: surface.into(),
            calibration_revision: descriptor.calibration_revision,
            confidence: 0.95,
            media: if mode == "frame" { Some(id) } else { None },
            projected_content: false,
        };
        let command = match mode {
            "observe" => Command::Observe { evidence },
            "frame" => Command::Frame {
                evidence,
                jpeg: BASE64_STANDARD.encode(bounded_file(
                    arguments
                        .get(4)
                        .ok_or("Provide a JPEG path for frame mode.")?,
                    524_288,
                )?),
            },
            "drag" => Command::Interact {
                interaction: Interaction {
                    evidence,
                    pointer: "rust-adapter-example".into(),
                    phase: if step == 0 { Phase::Begin } else { Phase::End },
                    camera_normalized: if step == 0 { [0.4, 0.4] } else { [0.5, 0.5] },
                },
            },
            _ => unreachable!(),
        };
        reply = exchange(
            &mut stream,
            Packet {
                version: PROTOCOL_VERSION,
                token: descriptor.token.clone(),
                sequence,
                command,
            },
        )
        .await?;
        if let Some(error) = &reply.error {
            return Err(error.clone());
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&reply).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            run(std::env::args().collect()),
        )
        .await
        .map_err(|_| "Adapter request timed out.".to_string())?
    })
}
