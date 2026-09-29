use std::io::{BufRead, Write};
use std::path::Path;

use engine::actions::Action;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Action { action: Action },
    Pair { code: String },
    Sync,
    Status,
    Quantity { record: String },
    Stop,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Response {
    Ready {
        build: String,
        organ: String,
        cell: String,
        node: String,
        pairing: String,
        nearby: Vec<String>,
    },
    Accepted {
        created: Option<String>,
    },
    Paired {
        organ: String,
    },
    Synced {
        operations: usize,
    },
    Quantity {
        value: Option<nucleus::DecimalValue>,
    },
    Refused {
        code: Option<String>,
        message: String,
    },
    Stopped,
}

pub fn run(directory: &Path) -> crate::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let cell = cell::Cell::open_mobile(cell::CellOptions {
            data_dir: Some(directory.to_path_buf()),
            peer_port: Some(0),
            ..Default::default()
        })
        .await?;
        cell.engine()
            .act(
                Action::SetCellConfig {
                    namespace: "lince.discovery".into(),
                    fds: serde_json::json!({"local":true,"internet":false,"direct":false,"accept_unknown":true}),
                },
                None,
            )
            .await?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if cell
                .runtime()
                .wire
                .read()
                .await
                .as_ref()
                .is_some_and(|wire| {
                    wire.local_discovery() && wire.reach() == engine::wire::Reach::Local
                })
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("worker local network did not start".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        cell.engine().create_organ_identity().await?;
        let organ = store::organs::local(&cell.engine().store.pool)
            .await?
            .ok_or("worker Organ missing")?;
        let signer = cell.engine().operational_key_for(&organ.uid).await?;
        cell.engine().set_signer(signer).await?;
        reply(&handle(&cell, Request::Status).await?)?;
        for line in std::io::stdin().lock().lines() {
            let line = line?;
            if line.len() > 1024 * 1024 {
                reply(&Response::Refused {
                    code: Some("request_too_large".into()),
                    message: "worker request exceeds one MiB".into(),
                })?;
                continue;
            }
            let request: Request = match serde_json::from_str(&line) {
                Ok(request) => request,
                Err(error) => {
                    reply(&Response::Refused {
                        code: Some("invalid_request".into()),
                        message: error.to_string(),
                    })?;
                    continue;
                }
            };
            if matches!(request, Request::Stop) {
                reply(&Response::Stopped)?;
                break;
            }
            let response = match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                handle(&cell, request),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => Response::Refused {
                    code: None,
                    message: error.to_string(),
                },
                Err(_) => Response::Refused {
                    code: Some("host_timeout".into()),
                    message: "worker operation timed out".into(),
                },
            };
            reply(&response)?;
        }
        cell.shutdown().await;
        Ok(())
    })
}

async fn handle(cell: &cell::Cell, request: Request) -> crate::Result<Response> {
    let engine = cell.engine();
    Ok(match request {
        Request::Action { action } => {
            crate::scenario::validate_action(&action)?;
            match engine.act(action, None).await {
                Ok(outcome) => Response::Accepted {
                    created: outcome.created,
                },
                Err(error) => Response::Refused {
                    code: error.code().map(String::from),
                    message: error.to_string(),
                },
            }
        }
        Request::Pair { code } => {
            let invite = engine::pairing::PairingInvite::decode(&code)?;
            let wire = cell
                .runtime()
                .wire
                .read()
                .await
                .clone()
                .ok_or("worker network unavailable")?;
            Response::Paired {
                organ: wire.pair_with(&invite, "Simulation peer").await?,
            }
        }
        Request::Sync => Response::Synced {
            operations: engine.sync_now().await?,
        },
        Request::Status => {
            let organ = store::organs::local(&engine.store.pool)
                .await?
                .ok_or("worker Organ missing")?;
            let local = store::cells::local(&engine.store.pool)
                .await?
                .ok_or("worker Cell missing")?;
            let wire = cell
                .runtime()
                .wire
                .read()
                .await
                .clone()
                .ok_or("worker network unavailable")?;
            Response::Ready {
                build: crate::BUILD_HASH.into(),
                organ: organ.uid,
                cell: local.uid,
                node: wire.node_id().to_string(),
                pairing: wire.pairing_invite().await?.encode(),
                nearby: engine
                    .nearby_peers()
                    .into_iter()
                    .map(|peer| peer.node_id)
                    .collect(),
            }
        }
        Request::Quantity { record } => Response::Quantity {
            value: store::records::get(&engine.store.pool, &record)
                .await?
                .map(|record| record.quantity),
        },
        Request::Stop => Response::Stopped,
    })
}

fn reply(response: &Response) -> crate::Result<()> {
    println!("{}", serde_json::to_string(response)?);
    std::io::stdout().flush()?;
    Ok(())
}
