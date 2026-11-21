// non-lean-yet
// Scope: struct Fixture, enum Contents, mod tests, fn run.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (256).

use super::*;

#[derive(Clone, Copy)]
pub(super) struct Fixture {
    pub rows: usize,
    pub contents: Contents,
}

#[derive(Clone, Copy, Default)]
pub(super) enum Contents {
    #[default]
    Repeated,
    Distinct,
}

impl Contents {
    fn name(self) -> &'static str {
        match self {
            Self::Repeated => "repeated-paragraphs",
            Self::Distinct => "distinct-paragraphs",
        }
    }

    pub(super) fn body(self, row: usize) -> String {
        const PARAGRAPH: &str =
            "**Private fixture**: rich text, Unicode café 日本語, and local data.\n\n";
        match self {
            Self::Repeated => PARAGRAPH.repeat(100),
            Self::Distinct => (0..100)
                .map(|paragraph| {
                    PARAGRAPH
                        .replace(
                            "Private fixture",
                            &format!("Record {row:03} {paragraph:03}!"),
                        )
                        .replace(
                            "local data.",
                            &format!("data {:05}.", row * 100 + paragraph),
                        )
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::Contents;

    #[test]
    fn distinct_fixture_keeps_paragraph_load_without_duplicate_paragraphs() {
        let repeated = Contents::Repeated.body(0);
        let mut paragraphs = HashSet::new();
        for row in 0..300 {
            let distinct = Contents::Distinct.body(row);
            assert_eq!(distinct.len(), repeated.len());
            assert_eq!(distinct.chars().count(), repeated.chars().count());
            assert_eq!(distinct.matches("**").count(), 200);
            assert_eq!(distinct.matches("café 日本語").count(), 100);
            let body: Vec<_> = distinct.split_terminator("\n\n").collect();
            assert_eq!(body.len(), 100);
            for paragraph in body {
                assert!(paragraphs.insert(paragraph.to_owned()));
            }
        }
        assert_eq!(paragraphs.len(), 30_000);
        assert_eq!(Contents::Repeated.body(299), repeated);
    }
}

pub fn run() -> io::Result<()> {
    if !performance::enabled() || std::env::var_os("LINCE_PERFORMANCE_LOG").is_none() {
        return Err(error(
            "Projection reveal requires retained performance measurements",
        ));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()?;
    let native = interface()?;
    println!("{native}");
    if native["passed"] != true {
        return Err(error("Required native interface checks failed"));
    }
    let mut results = Vec::new();
    let measured = (|| -> io::Result<()> {
        let fixtures = [1, 20, 200]
            .map(|rows| Fixture {
                rows,
                contents: Contents::Repeated,
            })
            .into_iter()
            .chain([Fixture {
                rows: 200,
                contents: Contents::Distinct,
            }]);
        for fixture in fixtures {
            eprintln!(
                "Starting populated projection reveal: {} rows, {}",
                fixture.rows,
                fixture.contents.name()
            );
            let result = scene(&runtime, "projection-reveal", true, false, Some(fixture))?;
            println!("{result}");
            results.push(result);
        }
        performance::set_phase(Phase::Warmup);
        crate::presentation::probe().map_err(error)?;
        Ok(())
    })();
    let drained = performance::finish(Duration::from_secs(3));
    let complete = measured.is_ok() && results.iter().all(|result| result["completed"] == true);
    println!(
        "{}",
        json!({
            "suite":"park-projection-reveal-v2",
            "scope":"diagnostic; native input to completed GPU frame, not physical scanout",
            "optimized":!cfg!(debug_assertions),
            "revision":option_env!("LINCE_REVISION").unwrap_or("unknown"),
            "native":native,
            "pixels_and_output_mapping":measured.is_ok(),
            "completed":complete,
            "trace_drained":drained,
            "error":measured.as_ref().err().map(ToString::to_string),
            "cases":results,
        })
    );
    measured?;
    if !drained {
        return Err(error("Projection measurements did not finish draining"));
    }
    if !complete {
        return Err(error(
            "Populated projection exceeded its bounded diagnostic",
        ));
    }
    Ok(())
}

pub(super) fn measure(
    canvas: &mut Presentation,
    root: Entity,
    owner: Entity,
    fixture: Fixture,
    cold_ms: f64,
) -> io::Result<Value> {
    let count = fixture.rows;
    let item = native::sand_store::spawn_sand(
        canvas.world_mut(),
        root,
        2,
        native::sand_store::SandKind::EditableText,
        "",
        DVec2::ZERO,
    );
    let editor = {
        let world = canvas.world_mut();
        world
            .query::<(Entity, &bevy::text::EditableText)>()
            .iter(world)
            .find_map(|(entity, _)| {
                let mut current = entity;
                loop {
                    if current == item {
                        return Some(entity);
                    }
                    current = world.get::<ChildOf>(current)?.parent();
                }
            })
            .ok_or_else(|| error("Projection fixture has no native editor"))?
    };
    for _ in 0..8 {
        completed(canvas)?;
    }
    canvas.focus(true).map_err(error)?;
    let point = input_point(canvas, editor)?;
    canvas.pointer(point).map_err(error)?;
    completed(canvas)?;
    canvas
        .button(bevy::input::mouse::MouseButton::Left, true)
        .map_err(error)?;
    completed(canvas)?;
    canvas
        .button(bevy::input::mouse::MouseButton::Left, false)
        .map_err(error)?;
    completed(canvas)?;
    if canvas.world_mut().resource::<InputFocus>().get() != Some(editor) {
        return Err(error(
            "Projection fixture editor did not receive pointer focus",
        ));
    }
    performance::set_phase(Phase::OpenReveal);
    let revealed = Instant::now();
    canvas
        .move_workspace_to_output(3, OutputId(1))
        .map_err(error)?;
    canvas.focus_output(OutputId(2)).map_err(error)?;
    if canvas.world_mut().resource::<InputFocus>().get() != Some(editor) {
        return Err(error(
            "Projection fixture did not restore the other output editor",
        ));
    }
    let mut expected = String::new();
    let mut samples = Vec::new();
    let mut rows = Vec::new();
    let mut complete_ms = None;
    let mut after_complete = 0;
    while after_complete < 12 {
        if revealed.elapsed() > Duration::from_secs(90) || samples.len() >= 256 {
            break;
        }
        let started = Instant::now();
        canvas
            .key(
                KeyCode::KeyA,
                Key::Character("é".into()),
                Some("é".into()),
                true,
                false,
            )
            .map_err(error)?;
        canvas
            .key(
                KeyCode::KeyA,
                Key::Character("é".into()),
                None,
                false,
                false,
            )
            .map_err(error)?;
        let frame = completed(canvas)?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        expected.push('é');
        if canvas
            .world_mut()
            .get::<bevy::text::EditableText>(editor)
            .is_none_or(|text| text.value().to_string() != expected)
        {
            return Err(error(format!(
                "Projection reveal input mismatch: expected {expected:?}, actual {:?}, focus {:?}",
                canvas
                    .world_mut()
                    .get::<bevy::text::EditableText>(editor)
                    .map(|text| text.value().to_string()),
                canvas.world_mut().resource::<InputFocus>().get(),
            )));
        }
        let stats = native::protein_area::projection_stats(canvas.world_mut(), owner)
            .ok_or_else(|| error("Projection owner disappeared"))?;
        if !stats.ready || stats.records != count || stats.materialized > count {
            return Err(error("Projection query or row count changed unexpectedly"));
        }
        let outputs = &frame.geometry.as_ref().unwrap().outputs;
        if !outputs
            .iter()
            .any(|output| output.id == OutputId(1) && output.workspace == 3)
            || !outputs
                .iter()
                .any(|output| output.id == OutputId(2) && output.workspace == 2)
        {
            return Err(error(
                "Projection reveal published incorrect output assignments",
            ));
        }
        samples.push(elapsed);
        rows.push(stats.materialized);
        println!(
            "{}",
            json!({"projection_rows":count,"input":samples.len(),"input_ms":elapsed,"materialized":stats.materialized})
        );
        if stats.materialized == count {
            complete_ms.get_or_insert_with(|| revealed.elapsed().as_secs_f64() * 1000.0);
            after_complete += 1;
            performance::set_phase(Phase::Typing);
        }
    }
    Ok(json!({
        "case":"populated-projection-reveal",
        "rows":count,
        "contents":fixture.contents.name(),
        "completed":after_complete == 12,
        "cold_setup_ms":cold_ms,
        "all_rows_materialized_ms":complete_ms,
        "input_ms":samples,
        "materialized_per_input":rows,
        "input_order_and_output_assignments":true,
        "all_input_under_100ms":samples.iter().all(|sample| *sample <= 100.0),
    }))
}
