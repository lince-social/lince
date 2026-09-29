#![deny(warnings)]

use bytes::Bytes;
use octopii::sim_runtime::{SimRuntime, advance_time, reset};
use octopii::transport::{SimConfig, SimRouter};
use octopii::wal::wal::vfs;
use openraft::AsyncRuntime;
use std::collections::BTreeSet;
use std::future::Future;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

fn addresses() -> [SocketAddr; 4] {
    [4100, 4101, 4102, 4103].map(|port| SocketAddr::from(([127, 0, 0, 1], port)))
}

fn route(seed: u64) -> Vec<(u64, usize, usize, Vec<u8>)> {
    let router = SimRouter::new(SimConfig {
        seed,
        drop_rate: 0.1,
        min_delay_ms: 1,
        max_delay_ms: 20,
    });
    let peers = addresses();
    let epochs = peers.map(|address| router.register(address));
    let mut trace = Vec::new();
    for tick in 0u64..100 {
        if tick == 20 {
            router.add_partition(peers[..2].to_vec(), peers[2..].to_vec());
        }
        if tick == 40 {
            router.clear_faults();
        }
        for from in 0..4 {
            for to in 0..4 {
                if from != to {
                    router
                        .enqueue(
                            peers[from],
                            epochs[from],
                            peers[to],
                            epochs[to],
                            Bytes::copy_from_slice(&tick.to_le_bytes()),
                        )
                        .unwrap();
                }
            }
        }
        router.advance_time(1);
        router.deliver_ready();
        for to in 0..4 {
            for from in 0..4 {
                while let Some(data) = router
                    .recv_from(peers[to], epochs[to], peers[from])
                    .unwrap()
                {
                    trace.push((router.now_ms(), from, to, data.to_vec()));
                }
            }
        }
    }
    trace
}

#[test]
fn four_endpoint_router_replays_and_isolates_worlds() {
    let mut delivered = 0;
    for seed in 1..=100 {
        let first = route(seed);
        let second = route(seed);
        assert_eq!(first, second);
        delivered += first.len();
    }
    let first = SimRouter::new(SimConfig::default());
    let second = SimRouter::new(SimConfig::default());
    first.advance_time(100);
    assert_eq!(second.now_ms(), 0);
    println!(
        "router: 100 seeds replayed, 4 endpoints, {delivered} deliveries, world clocks independent"
    );
}

#[test]
fn restart_drops_messages_for_the_previous_epoch() {
    let router = SimRouter::new(SimConfig {
        min_delay_ms: 10,
        max_delay_ms: 10,
        ..Default::default()
    });
    let peers = addresses();
    let sender = router.register(peers[0]);
    let old = router.register(peers[1]);
    router
        .enqueue(peers[0], sender, peers[1], old, Bytes::from_static(b"old"))
        .unwrap();
    router.close(peers[1], old);
    let new = router.register(peers[1]);
    router.advance_time(10);
    router.deliver_ready();
    assert!(router.recv_from(peers[1], new, peers[0]).unwrap().is_none());
    router
        .enqueue(peers[0], sender, peers[1], new, Bytes::from_static(b"new"))
        .unwrap();
    router.advance_time(10);
    router.deliver_ready();
    assert_eq!(
        router.recv_from(peers[1], new, peers[0]).unwrap().unwrap(),
        b"new"[..]
    );
    println!("router: stale delivery discarded, delivery after restart succeeded");
}

struct RecordedWake(usize, Arc<Mutex<Vec<usize>>>);

impl Wake for RecordedWake {
    fn wake(self: Arc<Self>) {
        self.1.lock().unwrap().push(self.0);
    }
}

fn wake_order() -> Vec<usize> {
    reset(42, 0);
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut sleeps = Vec::new();
    for index in 0..16 {
        let waker = Waker::from(Arc::new(RecordedWake(index, order.clone())));
        let mut sleep = Box::pin(SimRuntime::sleep(Duration::from_millis(1)));
        assert_eq!(
            sleep.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
        sleeps.push(sleep);
    }
    assert_eq!(advance_time(Duration::from_millis(1)), 16);
    let observed = order.lock().unwrap().clone();
    observed
}

#[test]
fn measure_equal_deadline_wake_order() {
    let orders: BTreeSet<_> = (0..32)
        .map(|_| std::thread::spawn(wake_order).join().unwrap())
        .collect();
    println!(
        "clock: {} different equal-deadline wake orders across 32 fresh-thread runs with seed 42",
        orders.len()
    );
    assert_eq!(orders.iter().next().unwrap().len(), 16);
}

fn setup(seed: u64, errors: f64) {
    vfs::sim::setup(vfs::sim::SimConfig {
        seed,
        io_error_rate: errors,
        initial_time_ns: 1_700_000_000_000_000_000,
        enable_partial_writes: false,
    });
}

#[test]
fn measure_vfs_replay_and_same_thread_world_reset() {
    fn replay() -> Vec<u8> {
        setup(42, 0.0);
        let mut file = vfs::File::create("/virtual/probe").unwrap();
        file.write_all(b"durable").unwrap();
        file.sync_all().unwrap();
        let mut output = Vec::new();
        vfs::File::open("/virtual/probe")
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        vfs::sim::teardown();
        output
    }
    assert_eq!(replay(), replay());
    setup(1, 0.0);
    vfs::write("/virtual/world-a", b"a").unwrap();
    setup(2, 0.0);
    let first_world_survives = vfs::exists("/virtual/world-a");
    println!(
        "vfs: replay matched; first world survives second setup on same thread: {first_world_survives}"
    );
    vfs::sim::teardown();
}

#[test]
fn measure_worker_thread_and_sqlite_bypass() {
    let directory = tempfile::tempdir().unwrap();
    setup(42, 1.0);
    assert!(vfs::File::create(directory.path().join("blocked")).is_err());
    let worker_path = directory.path().join("worker");
    let target = worker_path.clone();
    let worker_active = std::thread::spawn(move || {
        let active = vfs::sim::is_active();
        vfs::File::create(target)
            .unwrap()
            .write_all(b"host")
            .unwrap();
        active
    })
    .join()
    .unwrap();
    let database = directory.path().join("sqlite.db");
    let sqlite = std::process::Command::new("sqlite3")
        .arg(&database)
        .arg("CREATE TABLE probe(value INTEGER); INSERT INTO probe VALUES(7); SELECT value FROM probe;")
        .output().unwrap();
    assert!(
        sqlite.status.success(),
        "{}",
        String::from_utf8_lossy(&sqlite.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&sqlite.stdout).trim(), "7");
    println!(
        "vfs: parent error rate 100%; worker simulation active: {worker_active}; worker wrote host file: {}; SQLite wrote host database: {}",
        worker_path.exists(),
        database.exists()
    );
    vfs::sim::teardown();
}
