// non-lean-yet
// Scope: struct Entry, struct Caption, fn caption, fn reconcile.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 27, 47, 121, 129,
//   139, 164.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (76, 105, 123, 131, 137, 152, 158, 175, 185, 189, 192).

use super::*;

pub(super) struct Entry {
    root: Entity,
    label: Entity,
    stop: Entity,
    content: Entity,
    open: bool,
    caption: Caption,
}

struct Caption {
    started_ms: u64,
    offset: Option<i32>,
    finished_ms: Option<u64>,
    exit_code: Option<u32>,
    error: Option<String>,
}

impl Caption {
    fn new(run: &Run, offset: Option<i32>) -> Self {
        Self {
            started_ms: run.started_ms,
            offset,
            finished_ms: run.finished_ms,
            exit_code: run.exit_code,
            error: run.error.clone(),
        }
    }

    fn matches(&self, run: &Run, offset: Option<i32>) -> bool {
        self.started_ms == run.started_ms
            && self.offset == offset
            && self.finished_ms == run.finished_ms
            && self.exit_code == run.exit_code
            && self.error == run.error
    }
}

fn caption(run: &Run, time: Option<chrono::DateTime<chrono::Local>>) -> String {
    let time = time
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let status = if run.finished_ms.is_none() {
        "Running".into()
    } else if let Some(error) = &run.error {
        error.clone()
    } else {
        format!(
            "Exit {}",
            run.exit_code
                .map_or("unknown".into(), |code| code.to_string())
        )
    };
    let duration = run
        .finished_ms
        .filter(|_| {
            !run.error
                .as_ref()
                .is_some_and(|error| error.starts_with("Interrupted"))
        })
        .map(|finished| {
            format!(
                " · {:.1}s",
                finished.saturating_sub(run.started_ms) as f64 / 1000.0
            )
        })
        .unwrap_or_default();
    format!("{time} · {status}{duration}")
}

pub(super) fn reconcile(world: &mut World, owner: Entity) {
    let _measurement =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeCommandHistory);
    let (history, runs, mut entries) = {
        let mut castle = world.get_mut::<CommandCastle>(owner).unwrap();
        (
            castle.history,
            std::mem::take(&mut castle.runs),
            std::mem::take(&mut castle.entries),
        )
    };
    let ids: std::collections::HashSet<_> = runs.iter().map(|run| run.id.as_str()).collect();
    entries.retain(|id, entry| {
        if ids.contains(id.as_str()) {
            true
        } else {
            world.despawn(entry.root);
            false
        }
    });
    let mut order = Vec::with_capacity(runs.len());
    for run in &runs {
        let time = chrono::DateTime::from_timestamp_millis(run.started_ms as i64)
            .map(|time| time.with_timezone(&chrono::Local));
        let offset = time.as_ref().map(|time| time.offset().local_minus_utc());
        if let Some(entry) = entries.get_mut(&run.id) {
            if !entry.caption.matches(run, offset) {
                panel::status(world, entry.label, caption(run, time));
                let display = if run.finished_ms.is_none() {
                    Display::Flex
                } else {
                    Display::None
                };
                let mut node = world.get_mut::<Node>(entry.stop).unwrap();
                if node.display != display {
                    node.display = display;
                }
                entry.caption = Caption::new(run, offset);
            }
            order.push(entry.root);
            continue;
        }
        let root = panel::column(world, history);
        let controls = panel::row(world, root);
        let button = panel::button(
            world,
            controls,
            owner,
            &caption(run, time),
            Control::Toggle(run.id.clone()),
        );
        let label = world.get::<Children>(button).unwrap()[0];
        let stop = panel::button(
            world,
            controls,
            owner,
            "Stop",
            Control::Stop(run.id.clone()),
        );
        world.get_mut::<Node>(stop).unwrap().display = if run.finished_ms.is_none() {
            Display::Flex
        } else {
            Display::None
        };
        let content = panel::column(world, root);
        world.get_mut::<Node>(content).unwrap().display = Display::None;
        entries.insert(
            run.id.clone(),
            Entry {
                root,
                label,
                stop,
                content,
                open: false,
                caption: Caption::new(run, offset),
            },
        );
        order.push(root);
    }
    crate::native::retained::order(world, history, order);
    let mut castle = world.get_mut::<CommandCastle>(owner).unwrap();
    castle.runs = runs;
    castle.entries = entries;
}

pub(super) fn toggle(world: &mut World, owner: Entity, id: &str) -> Result<(), String> {
    let castle = world.get::<CommandCastle>(owner).unwrap();
    let run = castle
        .runs
        .iter()
        .find(|run| run.id == id)
        .ok_or("Run was not found")?
        .clone();
    let entry = castle.entries.get(id).ok_or("Run was not found")?;
    let (content, open) = (entry.content, !entry.open);
    if open {
        crate::native::edit_mode::label(
            world,
            content,
            &format!("Local machine · {}", run.cwd),
            12.0,
        );
        let script = panel::column(world, content);
        world.get_mut::<Node>(script).unwrap().max_height = px(140);
        crate::native::scroll_sand::attach(world, script);
        crate::native::edit_mode::label(world, script, &run.script, 13.0);
        if let Err(error) = crate::native::terminal::attach(world, content, &run) {
            panel::clear(world, content);
            return Err(error);
        }
    } else {
        panel::clear(world, content);
    }
    world.get_mut::<Node>(content).unwrap().display =
        if open { Display::Flex } else { Display::None };
    world
        .get_mut::<CommandCastle>(owner)
        .unwrap()
        .entries
        .get_mut(id)
        .unwrap()
        .open = open;
    Ok(())
}
