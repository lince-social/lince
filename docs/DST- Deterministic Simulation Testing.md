DST is amazing! The idea (I think) is to have three things:
        1. The Seed: the user's DNA (la ele).
        2. The Rules: What events should be bookmarked or stop the simulation?
        3. The Engine: How will this simulation happen? With the normal flow of time, or a tampered one? Connecting to the outside world with Commands?

        This way we can create futures shown to the user so they can see to the end of their Karma and catch bugs or unintended behavior.
        This is useful in finantial simulation, or for understanding the costs of time for doing tasks (like the Calendar feature).

        With DST we may duplicate the DNA to change it freely without affecting the user's data, or perhaps not changing persistent data at all,
        just manipulating data inside the program.

        TigerBeetle is the GOATED db for this, perhaps Lince can learn from it, fork it, or use it with a different schema for Transaction of Records.

        https://youtu.be/sC1B3d9C_sI?si=_HbNMQ9NVegLyS2a

        https://www.youtube.com/watch?v=JoYjji1DZCE


        Turso does not use a basic test script that just writes random data to different databases. Instead, Turso utilizes Deterministic Simulation Testing (DST) by completely abstracting the environment—including time, the network, and file system I/O—and replacing it with a pseudo-randomly seeded simulator. [1, 2, 3]
Because Turso is a ground-up rewrite of SQLite in Rust (originally under the repo name Limbo), they designed the core engine following "TigerStyle" software principles, ensuring that absolutely every background task can be controlled deterministically by a single PRNG seed. [3, 4, 5, 6]

---

## 📂 How It Is Structured

Turso's simulator code is organized inside their repository under their testing directories (such as testing/simulator/). It is broken into four distinct architectural layers: [2, 7, 8]

1.  Simulator (main.rs): The entry point. It generates random configuration setups and interaction plans, executing them sequentially or concurrently inside the runtime loop. [2, 7]
2.  Model (model.rs): A highly simplified, memory-resident representation of what the database should contain. It tracks atomic actions like insertions and selections to acts as a "source of truth". [2, 7]
3.  Generation (generation.rs): The code responsible for pseudo-randomly generating interaction plans, mock database tables, and schema workloads based on a configured workload distribution. [2, 7]
4.  Properties (properties.rs): Defines invariants and core database properties (like transaction atomicity, linearizability, or isolation levels). The engine checks these assertions at every step of the simulation loop. [2, 7, 9]

## 🛠 How the Simulation Logic Works

Turso avoids standard third-party Rust crates that interact directly with the operating system or system clock. Instead, the simulator operates through a strict architectural loop: [10]

- Complete I/O Mocking: The core database code doesn't make standard asynchronous calls directly to Linux io_uring or system threads during simulation. All network requests, file writes, and time delays flow through the simulation layer. [1, 10, 11]
- The Power of the Seed: The simulator generates an initial random seed. If an impossible-to-find, edge-case data corruption bug occurs after millions of randomized operations, developers can use that exact seed to replay the execution trace identical to how it failed. [1, 3]
- Fault Injection: Instead of just making normal writes, the simulator intentionally drops network packets, randomly pauses threads, delays storage commits, and shuts down simulated database nodes mid-write to stress-test the MVCC concurrent engine. [11, 12, 13]
- Dual Protection with Antithesis: Because a custom in-house simulator might have its own logical blind spots, Turso also pairs its DST framework with [Antithesis](https://antithesis.com/). Antithesis is a deterministic hypervisor that runs the compiled database in a virtualized environment to inject low-level OS/hardware faults and catch non-simulated I/O bugs. [14, 15, 16]

If you are interested in seeing how they implement this, you can browse the [Turso GitHub Repository](https://github.com/tursodatabase/turso) to look directly at the simulator's logic and the property invariants they test against. [2]
Would you like to explore how to write a basic deterministic state model in Rust, or would you prefer to look deeper into how Turso handles its async I/O loop inside the engine? [11, 17]

[1] [https://journal.resonatehq.io](https://journal.resonatehq.io/p/deterministic-simulation-testing)
[2] [https://github.com](https://github.com/tursodatabase/turso/blob/main/testing/simulator/README.md)
[3] [https://turso.tech](https://turso.tech/blog/a-deep-look-into-our-new-massive-multitenant-architecture)
[4] [https://turso.tech](https://turso.tech/blog/introducing-limbo-a-complete-rewrite-of-sqlite-in-rust)
[5] [https://github.com](https://github.com/tursodatabase/turso)
[6] [https://s2.dev](https://s2.dev/blog/dst)
[7] [https://github.com](https://github.com/tursodatabase/turso/blob/main/testing/simulator/README.md)
[8] [https://mohittalniya.medium.com](https://mohittalniya.medium.com/inside-the-vllm-semantic-router-a-deep-dive-into-intelligent-llm-routing-3e6b42e2a01d)
[9] [https://www.youtube.com](https://www.youtube.com/watch?v=E__g-Mck62U)
[10] [https://www.youtube.com](https://www.youtube.com/watch?v=MV0TNq6G5rk)
[11] [https://dev.to](https://dev.to/arshtechpro/turso-a-rust-rewrite-of-sqlite-setup-guide-and-whether-its-worth-your-time-16lk)
[12] [https://docs.turso.tech](https://docs.turso.tech/cloud/durability)
[13] [https://pierrezemb.fr](https://pierrezemb.fr/posts/learn-about-dst/)
[14] [https://turso.tech](https://turso.tech/blog/turso-the-next-evolution-of-sqlite)
[15] [https://turso.tech](https://turso.tech/blog/turso-the-next-evolution-of-sqlite)
[16] [https://github.com](https://github.com/tursodatabase/limbo/blob/main/CONTRIBUTING.md)
[17] [https://thenewstack.io](https://thenewstack.io/why-we-created-turso-a-rust-based-rewrite-of-sqlite/)
