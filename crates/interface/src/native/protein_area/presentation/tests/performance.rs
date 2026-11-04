use super::*;
use std::time::Instant;

fn cpu() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    let ticks = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
    ticks as f64 * 1000.0 / rustix::param::clock_ticks_per_second() as f64
}

#[test]
#[ignore = "warmed CPU-only Record projection diagnostic; run without competing compilation"]
fn retained_record_projection_cost_with_unchanged_rows_and_single_row_updates() {
    let mut samples = Vec::new();
    for count in [1, 32, 200] {
        let (mut app, _, owner) = super::super::super::tests::fixture();
        let config = Config {
            presentation: Some(template(
                "Projection diagnostic",
                Layout::Castle,
                &["head", "due_date", "quantity", "slug"],
            )),
            bindings: vec![Binding::new("head")],
            ..Default::default()
        };
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config.clone());
        app.world_mut().resource_mut::<Runtime>().areas.insert(owner, super::super::super::State {
            applied: Some(config), data: (0..count).map(|index| json!({
                "uid":format!("projection-{index}"), "head":format!("Record {index}"),
                "due_date":"2026-11-04", "quantity":index.to_string(), "slug":format!("slug-{index}"),
            })).collect(), ready:true, dirty:true, ..Default::default()
        });
        super::super::super::rows::reconcile(app.world_mut(), owner);
        for _ in 0..5 {
            app.world_mut().increment_change_tick();
            update(app.world_mut());
        }
        let entities = app.world().entities().len();
        for changing in [false, true] {
            let started_cpu = cpu();
            let mut elapsed = Vec::new();
            for round in 0..80 {
                app.world_mut().increment_change_tick();
                if changing {
                    let world = app.world_mut();
                    let row =
                        world.resource::<Runtime>().areas[&owner].row_entities["projection-0"];
                    world
                        .get_mut::<super::super::super::rows::Row>(row)
                        .unwrap()
                        .data["quantity"] = json!(round.to_string());
                }
                let started = Instant::now();
                update(app.world_mut());
                elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            assert_eq!(app.world().entities().len(), entities);
            if changing {
                let row =
                    app.world().resource::<Runtime>().areas[&owner].row_entities["projection-0"];
                let mut todo = vec![row];
                let mut visible = false;
                while let Some(entity) = todo.pop() {
                    visible |= app
                        .world()
                        .get::<Text>(entity)
                        .is_some_and(|text| text.0 == "79");
                    if let Some(children) = app.world().get::<Children>(entity) {
                        todo.extend(children.iter());
                    }
                }
                assert!(
                    visible,
                    "The derived quantity label did not follow the changed row"
                );
            }
            elapsed.sort_by(f64::total_cmp);
            samples.push(
                json!({"rows":count,"changing_one_row":changing,"rounds":elapsed.len(),
                "median_ms":elapsed[elapsed.len()/2],"p95_ms":elapsed[elapsed.len()*95/100],
                "total_ms":elapsed.iter().sum::<f64>(),"cpu_ms":cpu()-started_cpu}),
            );
        }
    }
    let result = json!({"scope":"Warmed CPU-only shared Record presentation derivation; diagnostic build, not full-frame or input-visible latency", "samples":samples});
    if let Some(path) = std::env::var_os("LINCE_RECORD_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("projection.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    eprintln!("record-projection: {result}");
}
