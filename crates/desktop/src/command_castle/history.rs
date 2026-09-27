use super::*;

pub(super) struct Entry {
    root: Entity,
    label: Entity,
    stop: Entity,
    content: Entity,
    open: bool,
}

fn caption(run: &Run) -> String {
    let time = chrono::DateTime::from_timestamp_millis(run.started_ms as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
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
    let castle = world.get::<CommandCastle>(owner).unwrap();
    let history = castle.history;
    let runs = castle.runs.clone();
    let removed: Vec<_> = castle
        .entries
        .keys()
        .filter(|id| !runs.iter().any(|run| &run.id == *id))
        .cloned()
        .collect();
    for id in removed {
        let entry = world
            .get_mut::<CommandCastle>(owner)
            .unwrap()
            .entries
            .remove(&id)
            .unwrap();
        world.despawn(entry.root);
    }
    let mut order = Vec::new();
    for run in &runs {
        if let Some(entry) = world
            .get::<CommandCastle>(owner)
            .unwrap()
            .entries
            .get(&run.id)
        {
            let (root, label, stop) = (entry.root, entry.label, entry.stop);
            panel::status(world, label, caption(run));
            world.get_mut::<Node>(stop).unwrap().display = if run.finished_ms.is_none() {
                Display::Flex
            } else {
                Display::None
            };
            order.push(root);
            continue;
        }
        let root = panel::column(world, history);
        let controls = panel::row(world, root);
        let button = panel::button(
            world,
            controls,
            owner,
            &caption(run),
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
        world
            .get_mut::<CommandCastle>(owner)
            .unwrap()
            .entries
            .insert(
                run.id.clone(),
                Entry {
                    root,
                    label,
                    stop,
                    content,
                    open: false,
                },
            );
        order.push(root);
    }
    world.entity_mut(history).replace_children(&order);
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
        crate::edit_mode::label(
            world,
            content,
            &format!("Local machine · {}", run.cwd),
            12.0,
        );
        let script = panel::column(world, content);
        world.get_mut::<Node>(script).unwrap().max_height = px(140);
        crate::scroll_sand::attach(world, script);
        crate::edit_mode::label(world, script, &run.script, 13.0);
        if let Err(error) = crate::terminal::attach(world, content, &run) {
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
