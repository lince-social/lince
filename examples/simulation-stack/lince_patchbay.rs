#![deny(warnings)]

use anyhow::{Context, Result, ensure};
use patchbay::{Device, Lab, LinkCondition, LinkDirection, RouterPreset};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Decimal {
    scale: u8,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Reply {
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
        value: Option<Decimal>,
    },
    Refused {
        code: Option<String>,
        message: String,
    },
    Stopped,
}

struct Peer {
    child: tokio::process::Child,
    input: tokio::process::ChildStdin,
    output: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    organ: String,
    cell: String,
    node: String,
    pairing: String,
    build: String,
}

impl Peer {
    async fn start(device: &Device, worker: &Path, directory: &Path) -> Result<Self> {
        let mut command = tokio::process::Command::new(worker);
        command
            .arg(directory)
            .env("LINCE_DISCOVERY_INTERNET", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = device.spawn_command(command)?;
        let input = child.stdin.take().context("worker stdin")?;
        let output = BufReader::new(child.stdout.take().context("worker stdout")?).lines();
        let mut peer = Self {
            child,
            input,
            output,
            organ: String::new(),
            cell: String::new(),
            node: String::new(),
            pairing: String::new(),
            build: String::new(),
        };
        let Reply::Ready {
            build,
            organ,
            cell,
            node,
            pairing,
            ..
        } = peer.reply().await?
        else {
            anyhow::bail!("worker did not start");
        };
        peer.build = build;
        peer.organ = organ;
        peer.cell = cell;
        peer.node = node;
        peer.pairing = pairing;
        Ok(peer)
    }

    async fn reply(&mut self) -> Result<Reply> {
        let line = tokio::time::timeout(Duration::from_secs(40), self.output.next_line())
            .await??
            .context("worker closed")?;
        Ok(
            serde_json::from_str(&line)
                .with_context(|| format!("invalid worker output: {line}"))?,
        )
    }

    async fn request(&mut self, request: serde_json::Value) -> Result<Reply> {
        self.input
            .write_all(serde_json::to_string(&request)?.as_bytes())
            .await?;
        self.input.write_all(b"\n").await?;
        self.input.flush().await?;
        self.reply().await
    }

    async fn action(&mut self, action: serde_json::Value) -> Result<Option<String>> {
        match self
            .request(json!({"kind":"action","action":action}))
            .await?
        {
            Reply::Accepted { created } => Ok(created),
            Reply::Refused { code, message } => anyhow::bail!("{code:?}: {message}"),
            reply => anyhow::bail!("unexpected action response {reply:?}"),
        }
    }

    async fn quantity(&mut self, record: &str) -> Result<Option<Decimal>> {
        match self
            .request(json!({"kind":"quantity","record":record}))
            .await?
        {
            Reply::Quantity { value } => Ok(value),
            reply => anyhow::bail!("unexpected quantity response {reply:?}"),
        }
    }

    async fn sync(&mut self) -> Result<()> {
        match self.request(json!({"kind":"sync"})).await? {
            Reply::Synced { operations } => {
                let _ = operations;
                Ok(())
            }
            Reply::Refused { .. } => Ok(()),
            reply => anyhow::bail!("unexpected sync response {reply:?}"),
        }
    }

    async fn stop(mut self) -> Result<()> {
        let _ = self.request(json!({"kind":"stop"})).await?;
        self.child.wait().await?;
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Check {
    Discovery {
        observer: String,
        peer: String,
        observed: Vec<String>,
        passed: bool,
    },
    Sync {
        source: String,
        target: String,
        record: String,
        expected: Decimal,
        observed: Option<Decimal>,
        passed: bool,
    },
    Partition {
        record: String,
        expected: Decimal,
        observed: Option<Decimal>,
        passed: bool,
    },
    Restart {
        expected_cell: String,
        observed_cell: String,
        expected_node: String,
        observed_node: String,
        passed: bool,
    },
}

#[derive(Serialize)]
struct Report {
    version: &'static str,
    coverage: &'static str,
    build: String,
    exact_replay: bool,
    checks: Vec<Check>,
    error: Option<String>,
}

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().collect();
    let worker = PathBuf::from(
        arguments
            .get(1)
            .context("provide the headless Lince worker path")?,
    )
    .canonicalize()?;
    let output = PathBuf::from(arguments.get(2).context("provide a new result directory")?);
    std::fs::create_dir(&output)?;
    let output = output.canonicalize()?;
    patchbay::init_userns()?;
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let mut report = Report { version: "lince.real-network.v1", coverage: "four-full-cells; same-LAN mDNS; outbound-to-public across two home NATs; partition; reconnect; durable restart", build: String::new(), exact_replay: false, checks: Vec::new(), error: None };
        let outcome = run(&worker, &output, &mut report).await;
        if let Err(error) = &outcome { report.error = Some(error.to_string()); }
        let file = std::fs::File::create(output.join("result.json"))?;
        serde_json::to_writer_pretty(&file, &report)?;
        file.sync_all()?;
        outcome
    })
}

async fn run(worker: &Path, output: &Path, report: &mut Report) -> Result<()> {
    let lab = Lab::new().await?;
    let public = lab
        .add_router("public")
        .preset(RouterPreset::Public)
        .build()
        .await?;
    let home_a = lab
        .add_router("home-a")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let home_b = lab
        .add_router("home-b")
        .preset(RouterPreset::Home)
        .build()
        .await?;
    let devices = [
        lab.add_device("public-cell")
            .iface("eth0", public.id())
            .build()
            .await?,
        lab.add_device("home-a-cell")
            .iface("eth0", home_a.id())
            .build()
            .await?,
        lab.add_device("home-b-cell-1")
            .iface("eth0", home_b.id())
            .build()
            .await?,
        lab.add_device("home-b-cell-2")
            .iface("eth0", home_b.id())
            .build()
            .await?,
    ];
    let mut peers = Vec::new();
    for (index, device) in devices.iter().enumerate() {
        peers.push(Peer::start(device, worker, &output.join(format!("cell-{index}"))).await?);
    }
    report.build = peers[0].build.clone();
    ensure!(
        peers.iter().all(|peer| peer.build == report.build),
        "workers do not share a build"
    );
    let mut observed = Vec::new();
    for _ in 0..80 {
        if let Reply::Ready { nearby, .. } = peers[2].request(json!({"kind":"status"})).await? {
            observed = nearby;
        }
        if observed.contains(&peers[3].node) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let passed = observed.contains(&peers[3].node);
    report.checks.push(Check::Discovery {
        observer: peers[2].node.clone(),
        peer: peers[3].node.clone(),
        observed,
        passed,
    });
    ensure!(passed, "full Cells did not discover their LAN peer");
    for index in 1..4 {
        let pairing = peers[0].pairing.clone();
        match peers[index]
            .request(json!({"kind":"pair","code":pairing}))
            .await?
        {
            Reply::Paired { organ } => ensure!(organ == peers[0].organ),
            reply => anyhow::bail!("pairing failed: {reply:?}"),
        }
        let organ = peers[index].organ.clone();
        peers[0]
            .action(json!({"action":"set-contact-trust","target":organ,"trust":"known"}))
            .await?;
        peers[0]
            .action(
                json!({"action":"set-sync-policy","target":organ,"sync_out":true,"sync_in":true}),
            )
            .await?;
        let public_organ = peers[0].organ.clone();
        peers[index].action(json!({"action":"set-sync-policy","target":public_organ,"sync_out":true,"sync_in":true})).await?;
        let record = peers[index].action(json!({"action":"create-record","slug":format!("network-{index}"),"kind":"plain","head":"Network work","body":"","quantity":10.0})).await?.context("created Record")?;
        let expected = Decimal {
            scale: 0,
            value: "10".into(),
        };
        let mut observed = None;
        for _ in 0..10 {
            peers[index].sync().await?;
            observed = peers[0].quantity(&record).await?;
            if observed.as_ref() == Some(&expected) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let passed = observed.as_ref() == Some(&expected);
        report.checks.push(Check::Sync {
            source: peers[index].cell.clone(),
            target: peers[0].cell.clone(),
            record,
            expected,
            observed,
            passed,
        });
        ensure!(passed, "Record did not converge through real Iroh");
    }
    let record = peers[1].action(json!({"action":"create-record","slug":"partition-record","kind":"plain","head":"Partition","body":"","quantity":1.0})).await?.context("partition Record")?;
    for _ in 0..10 {
        peers[1].sync().await?;
        if peers[0].quantity(&record).await?.is_some() {
            break;
        }
    }
    let iface = devices[1].default_iface().context("device interface")?;
    iface
        .set_condition(LinkCondition::new().loss_pct(100.0), LinkDirection::Both)
        .await?;
    peers[1]
        .action(json!({"action":"set-quantity-exact","target":record,"amount":"2"}))
        .await?;
    peers[1].sync().await?;
    let expected = Decimal {
        scale: 0,
        value: "1".into(),
    };
    let observed = peers[0].quantity(&record).await?;
    let passed = observed.as_ref() == Some(&expected);
    report.checks.push(Check::Partition {
        record: record.clone(),
        expected,
        observed,
        passed,
    });
    ensure!(passed, "partition failed to hold back the change");
    iface.clear_condition(LinkDirection::Both).await?;
    let expected = Decimal {
        scale: 0,
        value: "2".into(),
    };
    let mut observed = None;
    for _ in 0..10 {
        peers[1].sync().await?;
        observed = peers[0].quantity(&record).await?;
        if observed.as_ref() == Some(&expected) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let passed = observed.as_ref() == Some(&expected);
    report.checks.push(Check::Sync {
        source: peers[1].cell.clone(),
        target: peers[0].cell.clone(),
        record,
        expected,
        observed,
        passed,
    });
    ensure!(passed, "Cell did not reconnect after partition");
    let previous = peers.pop().context("last Cell")?;
    let expected_cell = previous.cell.clone();
    let expected_node = previous.node.clone();
    previous.stop().await?;
    let restarted = Peer::start(&devices[3], worker, &output.join("cell-3")).await?;
    let passed = restarted.cell == expected_cell && restarted.node == expected_node;
    report.checks.push(Check::Restart {
        expected_cell,
        observed_cell: restarted.cell.clone(),
        expected_node,
        observed_node: restarted.node.clone(),
        passed,
    });
    ensure!(passed, "restart changed the Cell identity");
    peers.push(restarted);
    for peer in peers {
        peer.stop().await?;
    }
    Ok(())
}
