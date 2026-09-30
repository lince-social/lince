use std::path::PathBuf;
use std::time::Instant;

use nucleus::simulation::{Evaluation, ReplayStatus, Verdict};

fn main() -> simulation::Result<()> {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run)?
        .join()
        .map_err(|_| "acceptance runner panicked")?
}

fn run() -> simulation::Result<()> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let cells: usize = args.first().ok_or("expected Cells Transfers output [full|selected|sampled|none] [replay]")?.parse()?;
        let transfers: usize = args.get(1).ok_or("expected Transfer count")?.parse()?;
        let output = PathBuf::from(args.get(2).ok_or("expected output directory")?);
        let mode = args.get(3).map_or("full", String::as_str);
        let mut case = simulation::fixtures::transfer::volume(cells, transfers)?;
        case.checking.on_failure = nucleus::simulation::FailureMode::Continue;
        match mode {
            "full" => {}
            "selected" => case.checks.retain(|check| check.id.starts_with("balance-")),
            "sampled" => for check in &mut case.checks { if check.id.starts_with("nonnegative-") { check.options.evaluation = Evaluation::EveryEvents { every: 100 }; } },
            "none" => case.checks.clear(),
            _ => return Err("unknown check mode".into()),
        }
        let started = Instant::now();
        let mut session = simulation::artifacts::Session::open(case, &output, std::path::Path::new(".")).await?;
        let mut progress = Instant::now();
        while session.step().await? {
            if progress.elapsed().as_secs() >= 20 { eprintln!("{cells} Cells/{transfers} Transfers: {} steps, {} events, {:.1}s, {:.1}s evidence", session.world.steps, session.world.trace.len(), started.elapsed().as_secs_f64(), session.world.evidence_micros as f64 / 1_000_000.0); progress = Instant::now(); }
        }
        let mut state = Vec::new();
        let mut metadata = std::collections::BTreeMap::new();
        let mut messages = 0;
        let mut recovery_steps = 0;
        for event in &session.world.trace {
            if matches!(event.observation, nucleus::simulation::Observation::MessageQueued { .. }) { messages += 1; }
            if matches!(&event.caused_by, nucleus::simulation::Cause::Input { id } if id.contains("-recovery-")) { recovery_steps += 1; }
        }
        for (name, node) in &session.world.nodes {
            let pool = &node.engine().store.pool;
            let canonical: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer").fetch_one(pool).await?;
            let applications: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_local_application").fetch_one(pool).await?;
            let hidden: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM visibility_rule WHERE grant_level = 'hidden'").fetch_one(pool).await?;
            let mut tables = std::collections::BTreeMap::new();
            let mut connection = pool.acquire().await?;
            let names: Vec<String> = store::sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name").fetch_all(&mut *connection).await?;
            for table in names.into_iter().filter(|name| store::transfer_replication::schema::allowed(name) && (name.starts_with("transfer") || name == "promise")) {
                let schema = store::transfer_replication::schema::Table::read(&mut connection, &table).await?;
                let query = format!("SELECT {} FROM {} ORDER BY {}", schema.json(""), store::transfer_replication::schema::quoted(&table), schema.keys.iter().map(|key| store::transfer_replication::schema::quoted(key)).collect::<Vec<_>>().join(","));
                let rows: Vec<String> = store::sqlx::query_scalar(&query).fetch_all(&mut *connection).await?;
                tables.insert(table, nucleus::karma::canonical_hash("lince.transfer.acceptance.table.v1", &rows)?);
            }
            metadata.insert(name.clone(), tables);
            let pending: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_delivery_outbox WHERE status IN ('queued','failed')").fetch_one(&mut *connection).await?;
            state.push(serde_json::json!({"cell":name,"canonical":canonical,"applications":applications,"hidden":hidden,"pending_deliveries":pending}));
        }
        let metadata_converged = (0..cells/4).all(|pair| ["a", "b"].into_iter().all(|side| metadata.get(&format!("p{pair}-{side}")) == metadata.get(&format!("p{pair}-{side}-replica"))));
        let final_state = session.world.state_hash().await?;
        let run = session.finish().await?;
        let wall_seconds = started.elapsed().as_secs_f64();
        let mut measured = serde_json::json!({"cells":cells,"transfers":transfers,"mode":mode,"wall_seconds":wall_seconds,"final_state":final_state,"result":run.result,"cost":run.cost,"state":state,"findings":run.findings,"messages":messages,"recovery_events":recovery_steps,"metadata_converged":metadata_converged,"metadata":metadata,"trace":simulation::artifacts::file_hash(&output.join("trace.jsonl"))?});
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") { measured["memory_high_water"] = status.lines().find(|line|line.starts_with("VmHWM:")).map_or(serde_json::Value::Null, |line|line.into()); }
        simulation::artifacts::atomic(&output.join("measurement.json"), &measured)?;
        println!("{}", serde_json::to_string(&serde_json::json!({"cells":cells,"transfers":transfers,"mode":mode,"wall_seconds":wall_seconds,"result":run.result,"cost":run.cost,"metadata_converged":metadata_converged,"memory_high_water":measured["memory_high_water"]}))?);
        if run.result.verdict != if mode == "none" { Verdict::Unverified } else { Verdict::Passed } { return Err("Transfer workload did not reach its expected verdict".into()); }
        if !metadata_converged { return Err("Transfer metadata differs after recovery".into()); }
        for pair in 0..cells/4 {
            let count = transfers / (cells/4) + usize::from(pair < transfers % (cells/4));
            for side in ["a","b"] {
                for suffix in ["", "-replica"] {
                    let name = format!("p{pair}-{side}{suffix}");
                    let row = state.iter().find(|row|row["cell"] == name).ok_or("missing Cell audit")?;
                    if row[if side == "a" {"canonical"} else {"applications"}] != count || row["hidden"].as_i64().unwrap_or(0) == 0 || row["pending_deliveries"] != 0 { return Err(format!("Transfer metadata did not converge on {name}: {row}").into()); }
                }
            }
        }
        if args.get(4).is_some_and(|arg|arg == "replay") {
            let started = Instant::now();
            let replay = simulation::artifacts::replay(&output, &output.with_extension("replay")).await?;
            simulation::artifacts::atomic(&output.join("replay-measurement.json"), &serde_json::json!({"wall_seconds":started.elapsed().as_secs_f64(),"replay":replay}))?;
            if !matches!(replay, ReplayStatus::Verified { .. }) { return Err("Transfer replay differs".into()); }
        }
        Ok(())
    })
}
