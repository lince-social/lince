#![deny(warnings)]

use anyhow::{Context, Result, ensure};
use iroh::endpoint::{NetReportConfig, PortmapperConfig, presets};
use iroh::{Endpoint, EndpointAddr, SecretKey};
use patchbay::{Device, Lab, LinkCondition, LinkDirection, RouterPreset};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

const ALPN: &[u8] = b"lince/simulation-stack-probe/1";

struct Request {
    destination: EndpointAddr,
    payload: Vec<u8>,
    response: oneshot::Sender<Result<Vec<u8>>>,
}

struct Node {
    address: EndpointAddr,
    requests: mpsc::Sender<Request>,
    task: tokio::task::JoinHandle<Result<()>>,
}

async fn exchange(
    endpoint: &Endpoint,
    destination: EndpointAddr,
    payload: Vec<u8>,
) -> Result<Vec<u8>> {
    let connection = endpoint.connect(destination, ALPN).await?;
    let (mut send, mut recv) = connection.open_bi().await?;
    send.write_all(&payload).await?;
    send.finish()?;
    let result = recv.read_to_end(4096).await?;
    connection.close(0u32.into(), b"done");
    Ok(result)
}

async fn start(device: &Device, seed: u8) -> Result<Node> {
    let bind = SocketAddr::from((device.ip().context("device address")?, 4400));
    let (ready_tx, ready_rx) = oneshot::channel();
    let (requests, mut rx) = mpsc::channel::<Request>(16);
    let task = device.spawn(move |_| async move {
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&[seed; 32]))
            .alpns(vec![ALPN.to_vec()])
            .portmapper_config(PortmapperConfig::Disabled)
            .net_report_config(NetReportConfig::minimal())
            .clear_ip_transports()
            .bind_addr(bind)?
            .bind().await?;
        let mdns = iroh_mdns_address_lookup::MdnsAddressLookup::builder()
            .service_name("lince")
            .build(endpoint.id())?;
        endpoint.address_lookup()?.add(mdns);
        ready_tx.send(EndpointAddr::new(endpoint.id()).with_ip_addr(bind)).map_err(|_| anyhow::anyhow!("receiver closed"))?;
        loop {
            tokio::select! {
                incoming = endpoint.accept() => {
                    let Some(incoming) = incoming else { break };
                    tokio::spawn(async move {
                        let connection = incoming.await?;
                        let (mut send, mut recv) = connection.accept_bi().await?;
                        let bytes = recv.read_to_end(4096).await?;
                        send.write_all(&bytes).await?;
                        send.finish()?;
                        let _ = connection.closed().await;
                        anyhow::Ok(())
                    });
                }
                request = rx.recv() => {
                    let Some(request) = request else { break };
                    let outcome = tokio::time::timeout(Duration::from_secs(3), exchange(&endpoint, request.destination, request.payload)).await;
                    let result = match outcome {
                        Ok(result) => result,
                        Err(error) => Err(error.into()),
                    };
                    let _ = request.response.send(result);
                }
            }
        }
        endpoint.close().await;
        Ok(())
    })?;
    let address = tokio::time::timeout(Duration::from_secs(10), ready_rx).await??;
    Ok(Node {
        address,
        requests,
        task,
    })
}

async fn request(from: &Node, to: &Node, payload: &[u8]) -> Result<Vec<u8>> {
    request_to(from, to.address.clone(), payload).await
}

async fn request_to(from: &Node, destination: EndpointAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let (response, result) = oneshot::channel();
    from.requests
        .send(Request {
            destination,
            payload: payload.to_vec(),
            response,
        })
        .await?;
    result.await?
}

fn main() -> Result<()> {
    patchbay::init_userns()?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<()> {
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
    let server = lab
        .add_device("server")
        .iface("eth0", public.id())
        .build()
        .await?;
    let a = lab
        .add_device("a")
        .iface("eth0", home_a.id())
        .build()
        .await?;
    let b = lab
        .add_device("b")
        .iface("eth0", home_b.id())
        .build()
        .await?;
    let c = lab
        .add_device("c")
        .iface("eth0", home_b.id())
        .build()
        .await?;
    let devices = [server, a, b, c];
    let mut nodes = Vec::new();
    for (index, device) in devices.iter().enumerate() {
        nodes.push(start(device, index as u8 + 1).await?);
    }
    let started = Instant::now();
    let mut delivered = 0;
    for round in 0..4 {
        for source in 1..4 {
            let payload = format!("round-{round}-node-{source}").into_bytes();
            ensure!(request(&nodes[source], &nodes[0], &payload).await? == payload);
            delivered += 1;
        }
    }
    println!(
        "iroh: 4 endpoints, 2 home NATs, {delivered} verified exchanges to public peer, elapsed_ms={}",
        started.elapsed().as_millis()
    );
    ensure!(
        request_to(
            &nodes[2],
            EndpointAddr::new(nodes[3].address.id),
            b"discovered"
        )
        .await?
            == b"discovered"
    );
    println!("iroh: same-LAN peer discovered by mDNS without supplying its IP address");
    let iface = devices[1].default_iface().context("interface")?;
    iface
        .set_condition(LinkCondition::new().loss_pct(100.0), LinkDirection::Both)
        .await?;
    let partition_failed = request(&nodes[1], &nodes[0], b"blocked").await.is_err();
    ensure!(partition_failed, "partition did not interrupt delivery");
    ensure!(request(&nodes[2], &nodes[0], b"independent").await? == b"independent");
    iface.clear_condition(LinkDirection::Both).await?;
    ensure!(request(&nodes[1], &nodes[0], b"healed").await? == b"healed");
    println!(
        "iroh: partition blocked one node; another remained connected; healed node reconnected"
    );
    let previous = nodes.remove(3);
    let identity = previous.address.id;
    drop(previous.requests);
    previous.task.await??;
    let restarted = start(&devices[3], 4).await?;
    ensure!(identity == restarted.address.id);
    ensure!(request(&restarted, &nodes[0], b"restart").await? == b"restart");
    nodes.push(restarted);
    println!("iroh: endpoint restart retained identity and delivered successfully");
    for node in nodes {
        drop(node.requests);
        node.task.await??;
    }
    Ok(())
}
