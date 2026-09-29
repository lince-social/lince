#![deny(warnings)]

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Delivery {
    node: usize,
    identity: u64,
    amount: i64,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Cell {
    quantity: i64,
    applied: BTreeSet<u64>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct World {
    time: u64,
    sequence: u64,
    random: u64,
    cells: [Cell; 4],
    queue: BTreeMap<(u64, u64), Delivery>,
    trace: Vec<(u64, u64, usize, u64, i64, &'static str)>,
}

impl World {
    fn random(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    fn schedule(&mut self, at: u64, delivery: Delivery) {
        assert!(at >= self.time);
        self.sequence += 1;
        self.queue.insert((at, self.sequence), delivery);
    }

    fn seeded(seed: u64) -> (Self, i64) {
        let mut world = Self {
            random: seed,
            ..Default::default()
        };
        let mut expected = 0;
        for identity in 0..256 {
            let amount = (world.random() % 19) as i64 - 9;
            expected += amount;
            for node in 0..4 {
                let at = world.random() % 60;
                let delivery = Delivery {
                    node,
                    identity,
                    amount,
                };
                world.schedule(at, delivery.clone());
                world.schedule(at, delivery.clone());
                world.schedule(100 + identity, delivery);
            }
        }
        (world, expected)
    }

    fn step(&mut self) -> bool {
        let Some(((at, sequence), delivery)) = self.queue.pop_first() else {
            return false;
        };
        self.time = at;
        let cell = &mut self.cells[delivery.node];
        let outcome = if delivery.node >= 2 && (10..40).contains(&at) {
            "partition"
        } else if cell.applied.insert(delivery.identity) {
            cell.quantity += delivery.amount;
            "applied"
        } else {
            "duplicate"
        };
        self.trace.push((
            at,
            sequence,
            delivery.node,
            delivery.identity,
            cell.quantity,
            outcome,
        ));
        true
    }

    fn finish(&mut self) {
        while self.step() {}
    }
}

#[test]
fn seeded_order_checkpoint_and_world_isolation() {
    let started = std::time::Instant::now();
    let mut event_count = 0;
    for seed in 1..=1000 {
        let (mut first, expected) = World::seeded(seed);
        let (mut second, _) = World::seeded(seed);
        let (untouched, _) = World::seeded(seed + 1);
        let initial_other_world = untouched.clone();
        for _ in 0..100 {
            assert!(first.step());
        }
        let mut checkpoint = first.clone();
        first.finish();
        checkpoint.finish();
        second.finish();
        assert_eq!(first, checkpoint);
        assert_eq!(first, second);
        assert_eq!(untouched, initial_other_world);
        for cell in &first.cells {
            assert_eq!(cell.quantity, expected);
            assert_eq!(cell.applied.len(), 256);
        }
        event_count += first.trace.len();
    }
    println!(
        "owned scheduler: 1000 seeds, 4 modeled cells, {event_count} events per replay pass, identical traces after replay/checkpoint, elapsed_ms={}",
        started.elapsed().as_millis()
    );
}
