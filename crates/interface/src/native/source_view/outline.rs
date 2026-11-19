use super::*;
use std::collections::BTreeSet;
#[path = "outline/tree.rs"]
mod tree;

#[derive(Component, Default)]
pub(super) struct Outline {
    expanded: BTreeSet<String>,
    closed: BTreeSet<String>,
    inspecting: BTreeSet<String>,
    all: bool,
    details: bool,
}

#[derive(Clone)]
pub(super) enum Control {
    Expand(String),
    Inspect(String),
    All,
    Details,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        capture(world, owner);
        let Some(data) = world
            .get::<View>(owner)
            .and_then(|view| view.preview.clone())
        else {
            return;
        };
        if let Self::Inspect(key) = self
            && data["files"]
                .as_array()
                .is_some_and(|files| files.iter().any(|file| file == key))
        {
            let Some(root) = root(world, owner) else {
                return;
            };
            let workspace = world
                .get::<crate::native::workspace::Workspaces>(root)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(root)
                .map_or(DVec2::ZERO, |view| view.center);
            let source = reference_source(world, owner);
            let preview = spawn(
                world,
                root,
                workspace,
                position,
                SourceView {
                    uri: key.clone(),
                    ..default()
                },
            );
            world.entity_mut(preview).insert((
                ReferenceSource(source),
                Outline {
                    inspecting: BTreeSet::from([key.clone()]),
                    ..default()
                },
            ));
            Operation::Read.apply(world, preview);
            return;
        }
        let mut outline = world.get_mut::<Outline>(owner).unwrap();
        match self {
            Self::All => {
                outline.all = !outline.all;
                outline.expanded.clear();
                outline.closed.clear();
            }
            Self::Details => outline.details = !outline.details,
            Self::Expand(key) => {
                if outline.all {
                    if !outline.closed.remove(key) {
                        outline.closed.insert(key.clone());
                    }
                } else if !outline.expanded.remove(key) {
                    outline.expanded.insert(key.clone());
                }
            }
            Self::Inspect(key) => {
                if !outline.inspecting.remove(key) {
                    outline.inspecting.insert(key.clone());
                }
            }
        }
        super::render(world, owner, data);
    }
}

fn bare(value: &str) -> String {
    lince_resources::address(value)
        .map(|mut uri| {
            uri.set_fragment(None);
            uri.to_string()
        })
        .unwrap_or_else(|_| value.into())
}

pub(super) fn render_tree(world: &mut World, owner: Entity, body: Entity, data: &Value) {
    paint(world, owner, body, tree::Tree::records(data));
}

fn paint(world: &mut World, owner: Entity, body: Entity, tree: tree::Tree<'_>) {
    let _prepare = nucleus::performance::Span::new("source-outline-row-preparation");
    if world.get::<Outline>(owner).is_none() {
        world.entity_mut(owner).insert(Outline::default());
    }
    let state = world.get::<Outline>(owner).unwrap();
    let all = state.all;
    let expanded = &state.expanded;
    let closed = &state.closed;
    let inspecting = &state.inspecting;
    let tree::Tree {
        nodes,
        roots,
        titles,
        content,
    } = tree;
    let mut pending = if all {
        let mut reachable = BTreeSet::new();
        let mut discover = roots.clone();
        while let Some(key) = discover.pop() {
            if reachable.insert(key.clone())
                && let Some(branch) = nodes.get(&key)
            {
                discover.extend(branch.children.iter().cloned());
            }
        }
        nodes
            .keys()
            .rev()
            .filter(|key| !reachable.contains(*key))
            .cloned()
            .map(|key| (key, 0usize))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    pending.extend(roots.into_iter().rev().map(|key| (key, 0usize)));
    let mut visited = BTreeSet::new();
    let mut rows = Vec::new();
    while let Some((key, depth)) = pending.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let Some(branch) = nodes.get(&key) else {
            continue;
        };
        let children = branch
            .children
            .iter()
            .filter(|child| nodes.contains_key(*child) && !visited.contains(*child))
            .collect::<Vec<_>>();
        let open = (all && !closed.contains(&key)) || expanded.contains(&key);
        let has_children = !children.is_empty();
        let inspect = inspecting.contains(&key);
        if open {
            pending.extend(
                children
                    .into_iter()
                    .rev()
                    .map(|child| (child.clone(), depth + 1)),
            );
        }
        rows.push((key, depth, has_children, open, inspect));
    }
    nucleus::performance::add("source-outline-rows-prepared", rows.len() as u64);
    drop(_prepare);
    for (key, depth, has_children, open, inspect) in rows {
        let tree::Branch { title, members, .. } = &nodes[&key];
        let row = panel::row(world, body);
        world.get_mut::<Node>(row).unwrap().padding.left = px(depth.min(20) as f32 * 16.0);
        if has_children {
            let button = panel::button(
                world,
                row,
                owner,
                if open { "▾" } else { "▸" },
                Control::Expand(key.clone()),
            );
            world
                .entity_mut(button)
                .insert(crate::native::icons::Tooltip(format!(
                    "{} children of {title}",
                    if open { "Collapse" } else { "Expand" }
                )));
        }
        let label = crate::native::edit_mode::label(world, row, title, 14.0);
        {
            let mut node = world.get_mut::<Node>(label).unwrap();
            node.flex_shrink = 1.0;
            node.flex_grow = 1.0;
            node.flex_basis = px(0);
            node.min_width = px(0);
            node.overflow = Overflow::clip();
        }
        world.entity_mut(label).insert((
            TextLayout::no_wrap(),
            crate::native::icons::Tooltip(title.clone()),
        ));
        if !members.is_empty() {
            let button = panel::button(world, row, owner, "👁", Control::Inspect(key.clone()));
            world
                .entity_mut(button)
                .insert(crate::native::icons::Tooltip(format!("Preview {title}")));
            if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(button)
            {
                accessibility.set_label(format!("Preview {title}"));
            }
        }
        if inspect {
            for index in members {
                match &content {
                    tree::Content::Records(records) => {
                        render_record(world, owner, body, &records[*index], &titles)
                    }
                    tree::Content::Files(files) => render_file(world, owner, body, files[*index]),
                }
            }
        }
    }
}

pub(super) fn controls(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    data: &Value,
    exchange: bool,
) {
    if world.get::<Outline>(owner).is_none() {
        world.entity_mut(owner).insert(Outline::default());
    }
    if exchange && data["records"].is_array() {
        let row = panel::row(world, parent);
        let complete = data["records"].as_array().is_some_and(|records| {
            !records.is_empty() && records.iter().all(|record| record["state"] == "reuse")
        });
        if complete {
            crate::native::edit_mode::label(world, row, "In Cell", 14.0);
            panel::button(world, row, owner, "Check for updates", Operation::Preview);
        } else {
            panel::button(world, row, owner, "Add to Cell", Operation::Pull);
        }
        if data["records"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["state"] == "update_available"))
        {
            panel::button(
                world,
                row,
                owner,
                "Accept source updates",
                Operation::Import(true),
            );
        }
    } else if data["browsing"] == true {
        panel::button(
            world,
            parent,
            owner,
            "Import through Sync",
            Operation::Exchange,
        );
    }
    let count = data["records"].as_array().map_or(0, Vec::len);
    if count > 0 {
        crate::native::edit_mode::label(
            world,
            parent,
            &format!("{count} Records · expand to explore, preview to read"),
            13.0,
        );
        let row = panel::row(world, parent);
        let all = world.get::<Outline>(owner).unwrap().all;
        panel::button(
            world,
            row,
            owner,
            if all { "Collapse all" } else { "Expand all" },
            Control::All,
        );
        if data["rules"]
            .as_array()
            .is_some_and(|rules| !rules.is_empty())
            || data["vocabulary"]
                .as_array()
                .is_some_and(|vocabulary| !vocabulary.is_empty())
        {
            panel::button(world, row, owner, "Rules and vocabulary", Control::Details);
        }
    }
    for conflict in data["conflicts"].as_array().into_iter().flatten() {
        crate::native::edit_mode::label(world, parent, &conflict.to_string(), 13.0);
    }
}

pub(super) fn details(world: &World, owner: Entity) -> bool {
    world
        .get::<Outline>(owner)
        .is_some_and(|state| state.details)
}

pub(super) fn render_files(world: &mut World, owner: Entity, body: Entity, data: &Value) {
    paint(world, owner, body, tree::Tree::files(data));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_add_action_previews_and_imports_the_entire_collection() {
        let directory = tempfile::tempdir().unwrap();
        utils::config::set_lince_data_dir_override(directory.path().join("data")).unwrap();
        let entry = directory.path().join("Root.lingua");
        std::fs::write(&entry, "Import \"Child.lingua\"\nRoot (@root: 0) { r_00000000000000000000009971\n} r_00000000000000000000009971\n").unwrap();
        std::fs::write(
            directory.path().join("Child.lingua"),
            "Child (@child: 0) { r_00000000000000000000009972\n} r_00000000000000000000009972\n",
        )
        .unwrap();
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let mut app = crate::native::sand_panel::tests::app();
        crate::native::laboratory::isolate(app.world_mut());
        crate::native::sand_panel::tests::connect(&mut app, engine.clone());
        app.add_plugins(SourceViewPlugin);
        let parent = app.world_mut().spawn(Node::default()).id();
        let owner = exchange(app.world_mut(), parent, entry.to_str().unwrap());
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            crate::native::description::source_sender(world, &Source::Local).is_some()
        })
        .await;
        Operation::Pull.apply(app.world_mut(), owner);
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            world
                .get::<View>(owner)
                .and_then(|view| view.preview.as_ref())
                .is_some_and(|data| {
                    data["records"].as_array().is_some_and(|records| {
                        records.len() == 2
                            && records.iter().all(|record| record["state"] == "reuse")
                    })
                })
        })
        .await;
        let count: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM record WHERE head IN ('Root', 'Child')",
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
        assert_eq!(count, 2);
        let visible = labels(app.world_mut());
        assert!(visible.iter().any(|text| text == "In Cell"));
        assert!(visible.iter().any(|text| text == "Root"));
        assert!(!visible.iter().any(|text| text == "Child"));
    }

    fn labels(world: &mut World) -> Vec<String> {
        world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.clone())
            .collect()
    }

    #[test]
    fn collection_stays_compact_until_children_or_content_are_requested() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let parent = world.spawn(Node::default()).id();
        let owner = exchange(world, parent, "https://example.org/Root.lingua");
        let records = (0..1000).map(|index| serde_json::json!({
            "uid":format!("r_{index}"), "base":format!("https://example.org/{index}.lingua"),
            "title":format!("Child {index}"),"body":format!("Long description {index}"),"state":"create"
        })).chain(std::iter::once(serde_json::json!({"uid":"r_root","base":"https://example.org/Root.lingua","title":"Philosophy","body":"Root description","state":"create"}))).collect::<Vec<_>>();
        let documents = (0..1000).map(|index| serde_json::json!({
            "uri":format!("https://example.org/{index}.lingua"),
            "children":if index == 0 {vec!["https://example.org/1.lingua"]} else {Vec::<&str>::new()}
        })).chain(std::iter::once(serde_json::json!({"uri":"https://example.org/Root.lingua","children":["https://example.org/0.lingua"]}))).collect::<Vec<_>>();
        render(
            world,
            owner,
            serde_json::json!({"entry":"https://example.org/Root.lingua","records":records,"documents":documents,"ready":true}),
        );
        let initial = labels(world);
        assert!(initial.iter().any(|text| text == "Add to Cell"));
        assert!(initial.iter().any(|text| text == "Philosophy"));
        assert!(initial.len() < 30);
        assert!(
            !initial
                .iter()
                .any(|text| text.starts_with("Child ") || text.contains("description"))
        );
        let view = world.get::<View>(owner).unwrap();
        assert_ne!(view.controls, view.body);
        assert!(world.get::<ScrollPosition>(view.body).is_some());
        Control::Expand("https://example.org/Root.lingua".into()).apply(world, owner);
        let first = labels(world);
        assert!(first.iter().any(|text| text == "Child 0"));
        assert!(!first.iter().any(|text| text == "Child 1"));
        Control::All.apply(world, owner);
        let expanded = labels(world);
        assert!(expanded.iter().any(|text| text == "Child 1"));
        assert!(expanded.iter().any(|text| text == "Child 999"));
        assert!(!expanded.iter().any(|text| text.contains("description")));
        Control::Expand("https://example.org/0.lingua".into()).apply(world, owner);
        let partial = labels(world);
        assert!(partial.iter().any(|text| text == "Child 0"));
        assert!(!partial.iter().any(|text| text == "Child 1"));
        assert!(partial.iter().any(|text| text == "Child 999"));
        Control::Expand("https://example.org/0.lingua".into()).apply(world, owner);
        Control::Inspect("https://example.org/Root.lingua".into()).apply(world, owner);
        assert!(labels(world).iter().any(|text| text == "Root description"));
        Control::All.apply(world, owner);
        assert!(!labels(world).iter().any(|text| text.starts_with("Child ")));
    }

    #[test]
    fn folder_listing_groups_nested_files_and_import_cycles_do_not_repeat_rows() {
        let mut app = crate::native::sand_panel::tests::app();
        let world = app.world_mut();
        let parent = world.spawn(Node::default()).id();
        let owner = exchange(world, parent, "file:///library");
        render(
            world,
            owner,
            serde_json::json!({"entry":"file:///library","directory":true,"files":["file:///library/Root.lingua","file:///library/parts/Child.lingua"],"records":[],"ready":false}),
        );
        Control::Expand("file:///library".into()).apply(world, owner);
        let visible = labels(world);
        assert!(visible.iter().any(|text| text == "parts"));
        assert!(!visible.iter().any(|text| text == "Child"));
        assert!(visible.iter().any(|text| text == "Add to Cell"));
        render(
            world,
            owner,
            serde_json::json!({"entry":"file:///library/Root.lingua","records":[{"uid":"r_a","base":"file:///library/Root.lingua","title":"Root"},{"uid":"r_b","base":"file:///library/Child.lingua","title":"Child"}],"documents":[{"uri":"file:///library/Root.lingua","children":["file:///library/Child.lingua"]},{"uri":"file:///library/Child.lingua","children":["file:///library/Root.lingua"]}]}),
        );
        Control::All.apply(world, owner);
        let all = labels(world);
        assert_eq!(all.iter().filter(|text| *text == "Root").count(), 1);
        assert_eq!(all.iter().filter(|text| *text == "Child").count(), 1);
    }
}
