use crate::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use nucleus::plugins::{
    Binding, Catalog, Command as Backend, Contribution, Direction, Field, FieldType,
};
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

#[derive(Component)]
struct View {
    status: Entity,
    list: Entity,
    editor: Entity,
    catalog: Option<Catalog>,
    form: Option<Form>,
}

struct Form {
    target: String,
    binding: Option<Binding>,
    fields: Vec<(Field, Entity)>,
    name: Option<Entity>,
    interval: Option<Entity>,
    filter: Option<Entity>,
    selection: Option<Entity>,
    preview_area: Entity,
    preview: Option<(Value, String)>,
}

#[derive(Component)]
struct Job(tokio::sync::oneshot::Receiver<Result<(Backend, Value), String>>);

#[derive(Component)]
struct Watch {
    receiver: tokio::sync::watch::Receiver<Option<Result<Catalog, String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone)]
enum Command {
    Run(Backend),
    Open(String, Option<Binding>),
    Protein(String),
    Direction(Direction),
    Preview,
    Save,
    Licenses(String),
    Close,
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::edit_mode::label(world, parent, text, 13.0)
}

pub(super) fn populate(world: &mut World, parent: Entity) {
    let owner = panel::column(world, parent);
    label(world, owner, "Plugins");
    label(
        world,
        owner,
        "Packages installed in this Cell’s plugins directory are discovered at startup. Enable only packages you trust: native plugins run with your computer account’s access. One package can provide Sync, Actions and Record schemas.",
    );
    let status = label(world, owner, "Loading installed packages…");
    let list = panel::column(world, owner);
    let editor = panel::column(world, owner);
    world.entity_mut(owner).insert(View {
        status,
        list,
        editor,
        catalog: None,
        form: None,
    });
}

fn snapshot(world: &World, owner: Entity) -> Result<(Value, Option<Binding>), String> {
    let form = world
        .get::<View>(owner)
        .and_then(|view| view.form.as_ref())
        .ok_or("Choose a contribution")?;
    let mut values = serde_json::Map::new();
    for (field, entity) in &form.fields {
        let text = panel::value(world, *entity)?;
        if !field.required && text.is_empty() {
            continue;
        }
        let value = match field.kind {
            FieldType::Text => json!(text),
            FieldType::Integer => json!(
                text.parse::<i64>()
                    .map_err(|_| format!("{} needs an integer", field.name))?
            ),
            FieldType::Boolean => json!(
                text.parse::<bool>()
                    .map_err(|_| format!("{} needs true or false", field.name))?
            ),
            FieldType::Json => serde_json::from_str(&text)
                .map_err(|_| format!("{} needs valid JSON", field.name))?,
        };
        values.insert(field.id.clone(), value);
    }
    let values = Value::Object(values);
    let binding = form
        .binding
        .clone()
        .map(|mut binding| {
            binding.settings = values.clone();
            binding.name = panel::value(world, form.name.ok_or("Missing Sync name")?)?;
            binding.interval_seconds =
                panel::value(world, form.interval.ok_or("Missing interval")?)?
                    .parse()
                    .map_err(|_| "Interval needs a number of seconds")?;
            let filter = form
                .filter
                .map(|entity| panel::value(world, entity))
                .transpose()?
                .unwrap_or_default();
            let mut filters = if simple_filter(&binding.incoming_filter) {
                Vec::new()
            } else {
                binding
                    .incoming_filter
                    .as_array()
                    .cloned()
                    .ok_or("Invalid saved incoming Protein")?
            };
            if !filter.trim().is_empty() {
                filters.push(json!({"text_contains":filter}));
            }
            binding.incoming_filter = json!(filters);
            binding.enabled = true;
            Ok::<_, String>(binding)
        })
        .transpose()?;
    Ok((values, binding))
}

fn simple_filter(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.is_empty()
            || (items.len() == 1
                && items[0].as_object().is_some_and(|item| {
                    item.len() == 1 && item.get("text_contains").is_some_and(Value::is_string)
                }))
    })
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let status = view.status;
        if crate::laboratory::active(world) {
            panel::status(
                world,
                status,
                "Executable plugins are unavailable in the Laboratory",
            );
            return;
        }
        if world.get::<Job>(owner).is_some() {
            return;
        }
        match self {
            Self::Open(target, binding) => {
                open(world, owner, target, binding.clone());
                return;
            }
            Self::Close => {
                let editor = view.editor;
                panel::clear(world, editor);
                world.get_mut::<View>(owner).unwrap().form = None;
                return;
            }
            Self::Protein(_) | Self::Direction(_) => {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    return;
                };
                let Some(form) = view.form.as_mut() else {
                    return;
                };
                let Some(binding) = form.binding.as_mut() else {
                    return;
                };
                match self {
                    Self::Protein(uid) => binding.protein_uid = Some(uid.clone()),
                    Self::Direction(direction) => binding.direction = direction.clone(),
                    _ => {}
                }
                form.preview = None;
                let caption = format!(
                    "Direction: {} · Protein: {}",
                    binding.direction.name(),
                    binding
                        .protein_uid
                        .as_deref()
                        .unwrap_or("Choose one for publication")
                );
                let selection = form.selection;
                if let Some(selection) = selection {
                    panel::status(world, selection, caption);
                }
                return;
            }
            Self::Licenses(id) => {
                let package = view
                    .catalog
                    .as_ref()
                    .and_then(|catalog| {
                        catalog
                            .packages
                            .iter()
                            .find(|package| &package.manifest.id == id)
                    })
                    .cloned();
                let editor = view.editor;
                panel::clear(world, editor);
                world.get_mut::<View>(owner).unwrap().form = None;
                if let Some(package) = package {
                    for credit in package.manifest.credits {
                        label(world, editor, &credit);
                    }
                    for license in package.manifest.licenses {
                        label(world, editor, &license.name);
                        label(world, editor, &license.text);
                    }
                    panel::button(world, editor, owner, "Close licenses", Self::Close);
                }
                return;
            }
            _ => {}
        }
        let request = match self {
            Self::Run(request) => Ok(request.clone()),
            Self::Preview => snapshot(world, owner).and_then(|(_, binding)| {
                binding
                    .map(|binding| Backend::Preview { binding })
                    .ok_or("Choose a Sync provider".into())
            }),
            Self::Save => snapshot(world, owner).and_then(|(arguments, binding)| {
                let form = world.get::<View>(owner).unwrap().form.as_ref().unwrap();
                match binding {
                    Some(binding) => {
                        let preview = if binding.direction.outgoing() {
                            let (snapshot, token) = form
                                .preview
                                .as_ref()
                                .ok_or("Preview the selected Records first")?;
                            if *snapshot
                                != serde_json::to_value(&binding).map_err(|e| e.to_string())?
                            {
                                return Err("Settings changed; preview again".into());
                            }
                            Some(token.clone())
                        } else {
                            None
                        };
                        Ok(Backend::SaveBinding { binding, preview })
                    }
                    None => Ok(Backend::Invoke {
                        contribution: form.target.clone(),
                        arguments,
                    }),
                }
            }),
            _ => return,
        };
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                panel::status(world, status, error);
                return;
            }
        };
        let Some(runtime) = crate::practice_cells::runtime(world, owner) else {
            panel::status(world, status, "Local Cell unavailable");
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            panel::status(world, status, "Runtime unavailable");
            return;
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        handle.spawn(async move {
            let result = runtime
                .engine
                .plugin_command(request.clone(), None)
                .await
                .map(|outcome| (request, outcome.data.unwrap_or(Value::Null)))
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        world.entity_mut(owner).insert(Job(receiver));
        panel::status(world, status, "Working…");
    }
}

fn open(world: &mut World, owner: Entity, target: &str, binding: Option<Binding>) {
    let view = world.get::<View>(owner).unwrap();
    let descriptor = view
        .catalog
        .as_ref()
        .and_then(|catalog| {
            catalog
                .packages
                .iter()
                .filter(|package| package.enabled)
                .find_map(|package| {
                    package
                        .manifest
                        .contributions
                        .iter()
                        .find(|item| format!("{}/{}", package.manifest.id, item.id()) == target)
                })
        })
        .cloned();
    let Some(descriptor) = descriptor else {
        return;
    };
    let catalog = view.catalog.clone().unwrap();
    let editor = view.editor;
    panel::clear(world, editor);
    label(world, editor, descriptor.name());
    let sync = matches!(descriptor, Contribution::Sync { .. });
    let binding = if sync {
        Some(binding.unwrap_or_else(|| Binding {
            uid: nucleus::new_uid("r"),
            provider: target.into(),
            name: descriptor.name().into(),
            settings: json!({}),
            protein_uid: None,
            incoming_filter: json!([]),
            direction: if matches!(descriptor, Contribution::Sync { incoming: true, .. }) {
                Direction::Incoming
            } else {
                Direction::Outgoing
            },
            interval_seconds: 60,
            enabled: false,
            revision: 0,
        }))
    } else {
        None
    };
    let fields = descriptor
        .fields()
        .iter()
        .map(|field| {
            let value = binding
                .as_ref()
                .and_then(|binding| binding.settings.get(&field.id))
                .unwrap_or(&field.default);
            let text = value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if value.is_null() {
                    String::new()
                } else {
                    value.to_string()
                }
            });
            (
                field.clone(),
                panel::field(world, editor, &field.name, &text),
            )
        })
        .collect();
    let mut form = Form {
        target: target.into(),
        binding: binding.clone(),
        fields,
        name: None,
        interval: None,
        filter: None,
        selection: None,
        preview_area: editor,
        preview: None,
    };
    if let Some(binding) = binding {
        form.name = Some(panel::field(world, editor, "Sync name", &binding.name));
        form.interval = Some(panel::field(
            world,
            editor,
            "Refresh interval in seconds (30–86400)",
            &binding.interval_seconds.to_string(),
        ));
        let filter = if simple_filter(&binding.incoming_filter) {
            binding.incoming_filter[0]["text_contains"]
                .as_str()
                .unwrap_or("")
        } else {
            label(
                world,
                editor,
                "Existing incoming Protein conditions are preserved. The text below adds a condition.",
            );
            ""
        };
        if matches!(descriptor, Contribution::Sync { incoming: true, .. }) {
            form.filter = Some(panel::field(
                world,
                editor,
                "Incoming Protein: text contains (empty keeps all returned notes)",
                filter,
            ));
        }
        form.selection = Some(label(
            world,
            editor,
            &format!(
                "Direction: {} · Protein: {}",
                binding.direction.name(),
                binding
                    .protein_uid
                    .as_deref()
                    .unwrap_or("Choose one for publication")
            ),
        ));
        let controls = panel::row(world, editor);
        for (direction, caption) in [
            (Direction::Incoming, "Receive"),
            (Direction::Outgoing, "Publish"),
            (Direction::Both, "Both"),
        ] {
            let supported = match &descriptor {
                Contribution::Sync {
                    incoming, outgoing, ..
                } => (!direction.incoming() || *incoming) && (!direction.outgoing() || *outgoing),
                _ => false,
            };
            if supported {
                panel::button(
                    world,
                    controls,
                    owner,
                    caption,
                    Command::Direction(direction),
                );
            }
        }
        let document = match &descriptor {
            Contribution::Sync { document, .. } => document.as_ref(),
            _ => None,
        };
        let description = match document.map(|format| format.source) {
            Some(nucleus::plugins::SnapshotSource::Schedule) => {
                "Choose a saved Schedule Protein. Its window, filters and sorting determine the exported calendar. Each Sync has its own directory and filename. External file edits are replaced on refresh. Incomplete simulations keep the last successful file."
            }
            Some(nucleus::plugins::SnapshotSource::Record) => {
                "Choose a saved Record Protein. Each Sync exports its selection to its own directory and filename. External file edits are replaced on refresh."
            }
            None => {
                "Outgoing publication uses a saved Record Protein, with at most 100 local plain Records. Received Records are excluded. Future matching Records also publish while enabled."
            }
        };
        label(world, editor, description);
        for (uid, name) in catalog.proteins {
            panel::button(
                world,
                editor,
                owner,
                &format!("Use Protein: {name}"),
                Command::Protein(uid),
            );
        }
        panel::button(world, editor, owner, "Preview selection", Command::Preview);
    }
    panel::button(
        world,
        editor,
        owner,
        if sync {
            "Save and start Sync"
        } else {
            "Run action"
        },
        Command::Save,
    );
    panel::button(world, editor, owner, "Close settings", Command::Close);
    form.preview_area = panel::column(world, editor);
    world.get_mut::<View>(owner).unwrap().form = Some(form);
}

fn render(world: &mut World, owner: Entity, catalog: Catalog) {
    let list = world.get::<View>(owner).unwrap().list;
    panel::clear(world, list);
    for diagnostic in &catalog.diagnostics {
        label(world, list, diagnostic);
    }
    if catalog.packages.is_empty() {
        label(
            world,
            list,
            "No installed plugins. Install the Nostr example with cargo xtask plugin-example <data-directory>, then restart Lince.",
        );
    }
    for package in &catalog.packages {
        let row = panel::column(world, list);
        label(
            world,
            row,
            &format!(
                "{} · version {} · {}",
                package.manifest.name,
                package.manifest.version,
                if package.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                }
            ),
        );
        panel::button(
            world,
            row,
            owner,
            if package.enabled {
                "Disable package"
            } else {
                "Enable trusted native package"
            },
            Command::Run(Backend::Enable {
                plugin: package.manifest.id.clone(),
                digest: package.digest.clone(),
                enabled: !package.enabled,
            }),
        );
        panel::button(
            world,
            row,
            owner,
            "Licenses and credits",
            Command::Licenses(package.manifest.id.clone()),
        );
        for contribution in &package.manifest.contributions {
            let target = format!("{}/{}", package.manifest.id, contribution.id());
            match contribution {
                Contribution::Schema { namespace, .. } => {
                    label(
                        world,
                        row,
                        &format!("Record schema: {} · {namespace}", contribution.name()),
                    );
                }
                _ if package.enabled => {
                    panel::button(
                        world,
                        row,
                        owner,
                        contribution.name(),
                        Command::Open(target, None),
                    );
                }
                _ => {
                    label(world, row, contribution.name());
                }
            }
        }
    }
    for item in &catalog.bindings {
        let row = panel::column(world, list);
        label(
            world,
            row,
            &format!(
                "{} · {} · {} pending · {} need attention",
                item.binding.name, item.status, item.pending, item.failed
            ),
        );
        panel::button(
            world,
            row,
            owner,
            "Edit / start",
            Command::Open(item.binding.provider.clone(), Some(item.binding.clone())),
        );
        if item.binding.enabled {
            panel::button(
                world,
                row,
                owner,
                "Refresh now",
                Command::Run(Backend::Run {
                    binding: item.binding.uid.clone(),
                }),
            );
            panel::button(
                world,
                row,
                owner,
                "Stop Sync",
                Command::Run(Backend::Stop {
                    binding: item.binding.uid.clone(),
                }),
            );
        }
    }
    for delivery in &catalog.deliveries {
        if matches!(delivery.state.as_str(), "failed" | "uncertain") {
            label(
                world,
                list,
                &format!(
                    "Delivery {} · {} · {}",
                    delivery.record, delivery.state, delivery.result
                ),
            );
            panel::button(
                world,
                list,
                owner,
                "Retry same prepared publication",
                Command::Run(Backend::Retry {
                    delivery: delivery.uid.clone(),
                }),
            );
        }
    }
    world.get_mut::<View>(owner).unwrap().catalog = Some(catalog);
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if world.get::<Watch>(owner).is_none() && !crate::laboratory::active(world) {
            if let (Some(runtime), Ok(handle)) = (
                crate::practice_cells::runtime(world, owner),
                tokio::runtime::Handle::try_current(),
            ) {
                let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
                let (sender, receiver) = tokio::sync::watch::channel(None);
                let task = handle.spawn(async move {
                    let mut changed = runtime.engine.watch_query_changes();
                    let mut timer = tokio::time::interval(std::time::Duration::from_secs(5));
                    loop {
                        tokio::select! { _=timer.tick()=>{},result=changed.changed()=>{if result.is_err(){break;}} }
                        if sender.send(Some(runtime.engine.plugin_catalog().await.map_err(|error| error.to_string()))).is_err() { break; }
                        if let Some(wake) = &wake { wake.ring(); }
                    }
                });
                world.entity_mut(owner).insert(Watch { receiver, task });
            }
        }
        let result = world
            .get_mut::<Job>(owner)
            .and_then(|mut job| match job.0.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(_) => Some(Err(
                    "Request interrupted; inspect the local outcome before retrying".into(),
                )),
            });
        if let Some(result) = result {
            world.entity_mut(owner).remove::<Job>();
            let status = world.get::<View>(owner).unwrap().status;
            match result {
                Ok((Backend::Preview { binding }, result)) => {
                    let token = result["preview"].as_str().unwrap_or("").to_owned();
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    if let Some(form) = &mut view.form {
                        form.preview =
                            Some((serde_json::to_value(binding).unwrap_or(Value::Null), token));
                        let parent = form.preview_area;
                        panel::clear(world, parent);
                        label(
                            world,
                            parent,
                            &format!(
                                "{} entries selected for {}",
                                result["count"],
                                if result["document"] == true {
                                    "file export"
                                } else {
                                    "public publication"
                                }
                            ),
                        );
                        if let Some(destination) = result["destination"].as_str() {
                            label(world, parent, destination);
                        }
                        for record in result["records"].as_array().into_iter().flatten() {
                            label(world, parent, record["head"].as_str().unwrap_or("Record"));
                            label(world, parent, record["body"].as_str().unwrap_or(""));
                            if let Some(start) = record["time"]["from_ms"]
                                .as_i64()
                                .and_then(chrono::DateTime::from_timestamp_millis)
                            {
                                label(world, parent, &start.to_rfc3339());
                            } else if let Some(date) = record["start_date"]
                                .as_str()
                                .or(record["due_date"].as_str())
                            {
                                label(world, parent, date);
                            }
                        }
                    }
                    panel::status(
                        world,
                        status,
                        if result["document"] == true {
                            "Review the selection and destination, then save and start Sync"
                        } else {
                            "Review the public content, then save and start Sync"
                        },
                    );
                }
                Ok((Backend::Invoke { .. }, result)) => panel::status(
                    world,
                    status,
                    format!("Action result: {}", result["result"]),
                ),
                Ok((Backend::SaveBinding { .. }, result)) => {
                    let editor = world.get::<View>(owner).unwrap().editor;
                    panel::clear(world, editor);
                    world.get_mut::<View>(owner).unwrap().form = None;
                    if let Ok(catalog) = serde_json::from_value(result) {
                        render(world, owner, catalog);
                    }
                    panel::status(world, status, "Sync saved and enabled");
                }
                Ok((_, result)) => {
                    if let Ok(catalog) = serde_json::from_value(result) {
                        render(world, owner, catalog);
                    }
                    panel::status(world, status, "Settings saved");
                }
                Err(error) => panel::status(world, status, error),
            }
        }
        let snapshot = world.get_mut::<Watch>(owner).and_then(|mut watch| {
            if watch.receiver.has_changed().unwrap_or(false) {
                watch.receiver.borrow_and_update().clone()
            } else {
                None
            }
        });
        if let Some(snapshot) = snapshot {
            match snapshot {
                Ok(catalog) => {
                    if world.get::<View>(owner).unwrap().catalog.is_none() {
                        let status = world.get::<View>(owner).unwrap().status;
                        panel::status(world, status, "Installed packages loaded");
                    }
                    render(world, owner, catalog);
                }
                Err(error) => {
                    let status = world.get::<View>(owner).unwrap().status;
                    panel::status(world, status, error);
                }
            }
        }
    }
}
