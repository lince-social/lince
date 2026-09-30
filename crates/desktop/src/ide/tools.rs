use super::*;
use lince_editor::{language, lsp};
#[cfg(test)]
mod tests;
use std::{
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    root: PathBuf,
    language: String,
    command: Vec<String>,
}

#[derive(Component)]
struct Panel {
    panel: Entity,
    fields: [Entity; 3],
    status: Entity,
    results: Entity,
    language: Option<String>,
    path: Option<PathBuf>,
    connected: BTreeSet<Key>,
    completion: Option<(Ticket, Vec<lsp::Completion>)>,
    diagnostics: Vec<lsp::Diagnostic>,
    diagnostic_stamp: Option<(PathBuf, u64, u64)>,
}

#[derive(Clone)]
struct Ticket {
    owner: Entity,
    path: PathBuf,
    identity: u64,
    revision: u64,
    selection: [usize; 2],
}

struct Session {
    client: lsp::Client,
    ready: bool,
    formatting: bool,
    failed: bool,
    sent: BTreeMap<PathBuf, (u64, u64, bool)>,
    waiting: BTreeMap<PathBuf, (u64, u64, Instant)>,
}

enum ToolResult {
    Format(Vec<lince_editor::Edit>),
    Lint(String),
}

struct Reply {
    ticket: Ticket,
    result: Result<ToolResult, String>,
}

#[derive(Resource)]
struct Hub {
    sessions: BTreeMap<Key, Session>,
    diagnostics: BTreeMap<(Key, PathBuf), (u64, u64, Vec<lsp::Diagnostic>)>,
    pending: BTreeMap<u64, (Key, Ticket)>,
    next: u64,
    sender: mpsc::SyncSender<Reply>,
    receiver: Mutex<mpsc::Receiver<Reply>>,
    jobs: BTreeMap<Entity, tokio::task::JoinHandle<()>>,
    wake_at: Option<Instant>,
}

impl Default for Hub {
    fn default() -> Self {
        let (sender, receiver) = mpsc::sync_channel(8);
        Self {
            sessions: BTreeMap::new(),
            diagnostics: BTreeMap::new(),
            pending: BTreeMap::new(),
            next: 0,
            sender,
            receiver: Mutex::new(receiver),
            jobs: BTreeMap::new(),
            wake_at: None,
        }
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        for job in self.jobs.values() {
            job.abort();
        }
    }
}

pub(super) fn panel(world: &mut World, owner: Entity) {
    let panel = crate::sand_panel::column(world, owner);
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.get_mut::<Node>(panel).unwrap().max_height = px(300);
    world.get_mut::<Node>(panel).unwrap().overflow = Overflow::scroll_y();
    crate::scroll_sand::attach(world, panel);
    crate::edit_mode::label(
        world,
        panel,
        "Installed tools · executable and arguments as a JSON array · [] disables a command",
        12.0,
    );
    crate::edit_mode::label(
        world,
        panel,
        "Use PATH or a full path. On NixOS use system/Home Manager packages, or launch Lince inside nix develop. No downloads.",
        12.0,
    );
    let fields = [
        "Language server",
        "Formatter (text through stdin)",
        "Linter (text through stdin)",
    ]
    .map(|caption| {
        crate::edit_mode::label(world, panel, caption, 12.0);
        crate::file_explorer::input(world, panel, caption, "[]", 16_384)
    });
    let controls = crate::sand_panel::row(world, panel);
    for (caption, control) in [
        ("Connect / Restart", actions::Control::Connect),
        ("Disconnect", actions::Control::Disconnect),
        ("Format with command", actions::Control::FormatCommand),
        ("Run linter", actions::Control::LintCommand),
    ] {
        crate::sand_panel::button(world, controls, owner, caption, control);
    }
    let status = crate::edit_mode::label(
        world,
        panel,
        "Open a supported file to configure its language",
        12.0,
    );
    let results = crate::sand_panel::column(world, panel);
    world.entity_mut(owner).insert(Panel {
        panel,
        fields,
        status,
        results,
        language: None,
        path: None,
        connected: BTreeSet::new(),
        completion: None,
        diagnostics: Vec::new(),
        diagnostic_stamp: None,
    });
}

fn context(world: &World, owner: Entity) -> Option<(PathBuf, PathBuf, &'static str)> {
    let ide = world.get::<Ide>(owner)?;
    let path = ide.active.clone()?;
    let language = language::detect(&path)?;
    let root = ide
        .explorer
        .roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
        .or_else(|| path.parent().map(PathBuf::from))?;
    Some((path, root, language))
}

fn message(world: &mut World, owner: Entity, text: impl Into<String>) {
    if let Some(panel) = world.get::<Panel>(owner) {
        crate::sand_panel::status(world, panel.status, text);
    }
}

fn configure(world: &mut World, owner: Entity) {
    let context = context(world, owner);
    let path = context.as_ref().map(|(path, _, _)| path.clone());
    let language = context.map(|(_, _, language)| language.to_owned());
    let panel = world.get::<Panel>(owner).unwrap();
    let stale = panel.path != path
        || panel
            .completion
            .as_ref()
            .is_some_and(|(ticket, _)| !current(world, ticket, true))
        || panel
            .diagnostic_stamp
            .as_ref()
            .is_some_and(|(path, identity, revision)| {
                world.resource::<Documents>().0.get(path).is_none_or(|doc| {
                    doc.buffer.identity() != *identity || doc.buffer.revision() != *revision
                })
            });
    if stale {
        let mut panel = world.get_mut::<Panel>(owner).unwrap();
        panel.path = path;
        panel.completion = None;
        panel.diagnostics.clear();
        panel.diagnostic_stamp = None;
        let results = panel.results;
        crate::sand_panel::clear(world, results);
    }
    if world.get::<Panel>(owner).unwrap().language == language {
        return;
    }
    let config = language
        .as_ref()
        .map(|language| {
            world
                .get::<Ide>(owner)
                .unwrap()
                .language_tools
                .get(language)
                .cloned()
                .unwrap_or_else(|| language::Tools::for_language(language))
        })
        .unwrap_or_default();
    let fields = world.get::<Panel>(owner).unwrap().fields;
    for (field, value) in fields
        .into_iter()
        .zip([config.server, config.formatter, config.linter])
    {
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(&serde_json::to_string(&value).unwrap());
    }
    message(
        world,
        owner,
        language.as_ref().map_or_else(
            || "No language tools configured for this file".into(),
            |language| format!("{language} · Connect uses the installed executable"),
        ),
    );
    let mut panel = world.get_mut::<Panel>(owner).unwrap();
    panel.language = language;
    panel.completion = None;
    panel.diagnostics.clear();
    panel.diagnostic_stamp = None;
    let results = panel.results;
    crate::sand_panel::clear(world, results);
}

fn config(world: &mut World, owner: Entity, language: &str) -> Result<language::Tools, String> {
    let fields = world.get::<Panel>(owner).unwrap().fields;
    let values: Result<Vec<Vec<String>>, String> = fields
        .into_iter()
        .map(|field| {
            let text = crate::sand_panel::value(world, field)?;
            serde_json::from_str(&text)
                .map_err(|_| "Use a JSON array such as [\"rust-analyzer\"]".into())
        })
        .collect();
    let mut values = values?.into_iter();
    let config = language::Tools {
        server: values.next().unwrap(),
        formatter: values.next().unwrap(),
        linter: values.next().unwrap(),
    };
    if !config.valid() {
        return Err("Tool configuration is too long".into());
    }
    world
        .get_mut::<Ide>(owner)
        .unwrap()
        .language_tools
        .insert(language.into(), config.clone());
    Ok(config)
}

fn ticket(world: &World, owner: Entity, path: PathBuf) -> Result<Ticket, String> {
    let view = world.get::<View>(owner).ok_or("Editor closed")?;
    let document = world
        .resource::<Documents>()
        .0
        .get(&path)
        .ok_or("Wait for the file to open")?;
    if view.draft
        || document.preview.is_some()
        || document.buffer.conflict().is_some()
        || world
            .get::<EditableText>(view.editor)
            .is_some_and(|input| input.is_composing())
    {
        return Err("Finish composing or resolve the file before using language tools".into());
    }
    if document.buffer.snapshot().len_bytes() > lince_editor::tooling::MAX_TOOL_BYTES {
        return Err("Language tools are limited to files up to 2 MiB".into());
    }
    Ok(Ticket {
        owner,
        path,
        identity: document.buffer.identity(),
        revision: document.buffer.revision(),
        selection: view.selection,
    })
}

fn current(world: &World, ticket: &Ticket, selection: bool) -> bool {
    world.get::<View>(ticket.owner).is_some_and(|view| {
        !view.draft
            && (!selection || view.selection == ticket.selection)
            && world
                .get::<EditableText>(view.editor)
                .is_none_or(|input| !input.is_composing())
    }) && world
        .get::<Ide>(ticket.owner)
        .is_some_and(|ide| ide.active.as_ref() == Some(&ticket.path))
        && world
            .resource::<Documents>()
            .0
            .get(&ticket.path)
            .is_some_and(|doc| {
                doc.preview.is_none()
                    && doc.buffer.conflict().is_none()
                    && doc.buffer.identity() == ticket.identity
                    && doc.buffer.revision() == ticket.revision
            })
}

pub(super) fn action(world: &mut World, owner: Entity, action: &actions::Control) {
    world.init_resource::<Hub>();
    if world.get::<Panel>(owner).is_none() {
        return;
    }
    configure(world, owner);
    if matches!(action, actions::Control::Tools) {
        let panel = world.get::<Panel>(owner).unwrap().panel;
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
        return;
    }
    editing::capture_one(world, owner);
    let Some((path, root, language)) = context(world, owner) else {
        status(world, owner, "Choose a file with a supported language");
        return;
    };
    let panel = world.get::<Panel>(owner).unwrap().panel;
    world.get_mut::<Node>(panel).unwrap().display = Display::Flex;
    let result = perform(world, owner, action, path, root, language);
    if let Err(error) = result {
        message(world, owner, error);
    }
}

fn perform(
    world: &mut World,
    owner: Entity,
    action: &actions::Control,
    path: PathBuf,
    root: PathBuf,
    language: &str,
) -> Result<(), String> {
    if let actions::Control::ChooseCompletion(index) = action {
        let Some((ticket, items)) = world.get::<Panel>(owner).unwrap().completion.clone() else {
            return Ok(());
        };
        if !current(world, &ticket, true) {
            return Err("Completion expired; request it again".into());
        }
        if let Some(item) = items.get(*index) {
            apply(world, &ticket, &item.edits)?;
        }
        world.get_mut::<Panel>(owner).unwrap().completion = None;
        let results = world.get::<Panel>(owner).unwrap().results;
        crate::sand_panel::clear(world, results);
        return Ok(());
    }
    if let actions::Control::Diagnostic(index) = action {
        let panel = world.get::<Panel>(owner).unwrap();
        let stamp = panel.diagnostic_stamp.as_ref();
        if world
            .resource::<Documents>()
            .0
            .get(&path)
            .is_some_and(|doc| {
                stamp == Some(&(path.clone(), doc.buffer.identity(), doc.buffer.revision()))
            })
        {
            if let Some(diagnostic) = panel.diagnostics.get(*index) {
                editing::select(world, owner, [diagnostic.position; 2], diagnostic.line);
            }
        }
        return Ok(());
    }
    if matches!(action, actions::Control::Disconnect) {
        world
            .get_mut::<Panel>(owner)
            .unwrap()
            .connected
            .retain(|key| key.root != root || key.language != language);
        let mut panel = world.get_mut::<Panel>(owner).unwrap();
        panel.completion = None;
        panel.diagnostic_stamp = None;
        panel.diagnostics.clear();
        let results = panel.results;
        crate::sand_panel::clear(world, results);
        message(world, owner, "Disconnected");
        return Ok(());
    }
    let ticket = ticket(world, owner, path.clone())?;
    let external_format = matches!(action, actions::Control::Format)
        && !world
            .get::<Panel>(owner)
            .unwrap()
            .connected
            .iter()
            .any(|key| {
                key.root == root
                    && key.language == language
                    && world
                        .resource::<Hub>()
                        .sessions
                        .get(key)
                        .is_some_and(|session| {
                            session.ready && session.formatting && !session.failed
                        })
            });
    if external_format
        || matches!(
            action,
            actions::Control::FormatCommand | actions::Control::LintCommand
        )
    {
        let config = config(world, owner, language)?;
        return run_command(
            world,
            ticket,
            root,
            if external_format || matches!(action, actions::Control::FormatCommand) {
                config.formatter
            } else {
                config.linter
            },
            external_format || matches!(action, actions::Control::FormatCommand),
        );
    }
    if matches!(action, actions::Control::Connect) {
        let config = config(world, owner, language)?;
        if config.server.is_empty() {
            return Err("Enter the installed language server command".into());
        }
        let key = Key {
            root,
            language: language.into(),
            command: config.server,
        };
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let mut hub = world.resource_mut::<Hub>();
        if hub.sessions.contains_key(&key) {
            hub.sessions.remove(&key);
            hub.pending.retain(|_, (pending, _)| pending != &key);
        }
        if hub.sessions.len() >= 4 {
            return Err("Disconnect an unused language server first (limit: 4)".into());
        }
        let client = lsp::Client::start(key.command.clone(), key.root.clone(), move || {
            if let Some(wake) = &wake {
                wake.ring();
            }
        })?;
        hub.sessions.insert(
            key.clone(),
            Session {
                client,
                ready: false,
                formatting: false,
                failed: false,
                sent: BTreeMap::new(),
                waiting: BTreeMap::new(),
            },
        );
        let mut panel = world.get_mut::<Panel>(owner).unwrap();
        panel
            .connected
            .retain(|old| old.root != key.root || old.language != key.language);
        panel.connected.insert(key);
        message(world, owner, "Starting language server…");
        return Ok(());
    }
    let key = world
        .get::<Panel>(owner)
        .unwrap()
        .connected
        .iter()
        .find(|key| key.root == root && key.language == language)
        .cloned()
        .ok_or("Connect the installed language server first")?;
    let text = world.resource::<Documents>().0[&path].buffer.snapshot();
    let mut hub = world.resource_mut::<Hub>();
    if hub.pending.len() >= 32 {
        return Err("Wait for the current language requests".into());
    }
    hub.next += 1;
    let token = hub.next;
    let session = hub
        .sessions
        .get_mut(&key)
        .filter(|session| session.ready && !session.failed)
        .ok_or("Language server is not ready; connect or restart it")?;
    session.client.send(lsp::Command::Sync {
        path: path.clone(),
        language: language.into(),
        text,
        identity: ticket.identity,
        revision: ticket.revision,
    })?;
    session
        .sent
        .insert(path.clone(), (ticket.identity, ticket.revision, true));
    session
        .client
        .send(if matches!(action, actions::Control::Complete) {
            lsp::Command::Complete {
                path,
                position: ticket.selection[1],
                token,
            }
        } else {
            lsp::Command::Format { path, token }
        })?;
    hub.pending.insert(token, (key, ticket));
    message(world, owner, "Waiting for language server…");
    Ok(())
}

fn apply(world: &mut World, ticket: &Ticket, edits: &[lince_editor::Edit]) -> Result<(), String> {
    if !current(world, ticket, false) {
        return Err("The file changed while the tool was running; run it again".into());
    }
    world
        .resource_mut::<Documents>()
        .0
        .get_mut(&ticket.path)
        .unwrap()
        .buffer
        .edit_batch(edits)?;
    let mut position = ticket.selection[1];
    for edit in edits.iter().rev() {
        if position >= edit.range.end {
            position = position - edit.range.len() + edit.text.chars().count();
        } else if position > edit.range.start {
            position = edit.range.start + edit.text.chars().count();
        }
    }
    let line = world.resource::<Documents>().0[&ticket.path]
        .buffer
        .snapshot()
        .char_to_line(position);
    editing::select(world, ticket.owner, [position; 2], line);
    message(
        world,
        ticket.owner,
        "Applied to buffer · Save writes it to disk · Undo restores it",
    );
    Ok(())
}

fn run_command(
    world: &mut World,
    ticket: Ticket,
    root: PathBuf,
    command: Vec<String>,
    format: bool,
) -> Result<(), String> {
    let hub = world.resource::<Hub>();
    if hub.jobs.contains_key(&ticket.owner) || hub.jobs.len() >= 4 {
        return Err("Wait for the current tool command".into());
    }
    let sender = hub.sender.clone();
    let text = world.resource::<Documents>().0[&ticket.path]
        .buffer
        .snapshot();
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let owner = ticket.owner;
    let task = lince_editor::tooling::spawn(async move {
        let result = lince_editor::tooling::run(command, root, text.clone())
            .await
            .and_then(|output| {
                if format {
                    if !output.success {
                        return Err(format!(
                            "Formatter failed: {}",
                            output.stderr.chars().take(4096).collect::<String>()
                        ));
                    }
                    if output.stdout.contains('\0') {
                        return Err("Formatter returned binary text".into());
                    }
                    Ok(ToolResult::Format(
                        lince_editor::Edit::between(&text.to_string(), &output.stdout, 0)
                            .into_iter()
                            .collect(),
                    ))
                } else {
                    Ok(ToolResult::Lint(format!(
                        "{}\n{}\n{}",
                        if output.success {
                            "Linter finished"
                        } else {
                            "Linter reported problems"
                        },
                        output.stdout.chars().take(8192).collect::<String>(),
                        output.stderr.chars().take(8192).collect::<String>()
                    )))
                }
            });
        let _ = sender.try_send(Reply { ticket, result });
        if let Some(wake) = wake {
            wake.ring();
        }
    })?;
    world.resource_mut::<Hub>().jobs.insert(owner, task);
    message(world, owner, "Running installed tool…");
    Ok(())
}

pub(super) fn update(world: &mut World) {
    world.init_resource::<Hub>();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Panel>>()
        .iter(world)
        .collect();
    let mut wanted: BTreeMap<Key, BTreeSet<PathBuf>> = BTreeMap::new();
    for owner in &owners {
        if crate::laboratory::suspended(world, *owner) {
            continue;
        }
        configure(world, *owner);
        let panel = world.get::<Panel>(*owner).unwrap();
        let Some(ide) = world.get::<Ide>(*owner) else {
            continue;
        };
        for key in &panel.connected {
            wanted.entry(key.clone()).or_default().extend(
                ide.paths
                    .iter()
                    .filter(|path| {
                        path.starts_with(&key.root)
                            && language::detect(path) == Some(key.language.as_str())
                    })
                    .cloned(),
            );
        }
    }
    let now = Instant::now();
    world.resource_scope(|world, mut hub: Mut<Hub>| {
        hub.jobs.retain(|owner, job| {
            if world.get::<Panel>(*owner).is_none() {
                job.abort();
                false
            } else {
                true
            }
        });
        hub.sessions.retain(|key, _| wanted.contains_key(key));
        hub.diagnostics
            .retain(|(key, path), (identity, revision, _)| {
                wanted.get(key).is_some_and(|paths| paths.contains(path))
                    && world
                        .resource::<Documents>()
                        .0
                        .get(path)
                        .is_some_and(|doc| {
                            doc.buffer.identity() == *identity && doc.buffer.revision() == *revision
                        })
            });
        hub.pending.retain(|_, (key, ticket)| {
            wanted.contains_key(key) && world.get::<Panel>(ticket.owner).is_some()
        });
        let replies: Vec<_> = hub
            .receiver
            .lock()
            .expect("tool replies")
            .try_iter()
            .collect();
        for reply in replies {
            hub.jobs.remove(&reply.ticket.owner);
            match reply.result {
                Ok(ToolResult::Format(edits)) => {
                    if let Err(error) = apply(world, &reply.ticket, &edits) {
                        message(world, reply.ticket.owner, error);
                    }
                }
                Ok(ToolResult::Lint(output)) => {
                    if current(world, &reply.ticket, false) {
                        message(world, reply.ticket.owner, output);
                    } else {
                        message(
                            world,
                            reply.ticket.owner,
                            "Linter results expired because the file changed",
                        );
                    }
                }
                Err(error) => message(world, reply.ticket.owner, error),
            }
        }
        let mut events = Vec::new();
        let mut due: Option<Instant> = None;
        for (key, session) in &mut hub.sessions {
            for event in session.client.drain() {
                events.push((key.clone(), event));
            }
            if !session.ready || session.failed {
                continue;
            }
            let paths = &wanted[key];
            for path in session
                .sent
                .keys()
                .filter(|path| !paths.contains(*path))
                .cloned()
                .collect::<Vec<_>>()
            {
                if session
                    .client
                    .send(lsp::Command::Close(path.clone()))
                    .is_ok()
                {
                    session.sent.remove(&path);
                    session.waiting.remove(&path);
                }
            }
            for path in paths {
                let Some(doc) = world.resource::<Documents>().0.get(path).filter(|doc| {
                    doc.preview.is_none()
                        && doc.buffer.snapshot().len_bytes()
                            <= lince_editor::tooling::MAX_TOOL_BYTES
                }) else {
                    continue;
                };
                let stamp = (doc.buffer.identity(), doc.buffer.revision());
                if session
                    .sent
                    .get(path)
                    .is_some_and(|sent| (sent.0, sent.1) == stamp)
                {
                    if session.sent[path].2
                        && !doc.buffer.is_dirty()
                        && session
                            .client
                            .send(lsp::Command::Saved(path.clone()))
                            .is_ok()
                    {
                        session.sent.get_mut(path).unwrap().2 = false;
                    }
                    continue;
                }
                let wait = session.waiting.entry(path.clone()).or_insert((
                    stamp.0,
                    stamp.1,
                    now + Duration::from_millis(400),
                ));
                if (wait.0, wait.1) != stamp {
                    *wait = (stamp.0, stamp.1, now + Duration::from_millis(400));
                }
                if wait.2 <= now {
                    if session
                        .client
                        .send(lsp::Command::Sync {
                            path: path.clone(),
                            language: key.language.clone(),
                            text: doc.buffer.snapshot(),
                            identity: stamp.0,
                            revision: stamp.1,
                        })
                        .is_ok()
                    {
                        session
                            .sent
                            .insert(path.clone(), (stamp.0, stamp.1, doc.buffer.is_dirty()));
                        session.waiting.remove(path);
                    } else {
                        wait.2 = now + Duration::from_millis(400);
                        due = Some(wait.2);
                    }
                } else {
                    due = Some(due.map_or(wait.2, |due| due.min(wait.2)));
                }
            }
        }
        for (key, event) in events {
            response(world, &mut hub, &owners, &key, event);
        }
        let cached: Vec<_> = owners
            .iter()
            .filter_map(|owner| {
                let panel = world.get::<Panel>(*owner)?;
                if panel.diagnostic_stamp.is_some() || panel.completion.is_some() {
                    return None;
                }
                let path = world.get::<Ide>(*owner)?.active.as_ref()?;
                panel.connected.iter().find_map(|key| {
                    hub.diagnostics.get(&(key.clone(), path.clone())).map(
                        |(identity, revision, items)| {
                            (
                                key.clone(),
                                lsp::Event::Diagnostics {
                                    path: path.clone(),
                                    identity: *identity,
                                    revision: *revision,
                                    items: items.clone(),
                                },
                            )
                        },
                    )
                })
            })
            .collect();
        for (key, event) in cached {
            response(world, &mut hub, &owners, &key, event);
        }
        if hub.wake_at.is_some_and(|deadline| deadline <= now) {
            hub.wake_at = None;
        }
        if let Some(due) = due.filter(|due| hub.wake_at.is_none_or(|old| *due < old)) {
            hub.wake_at = Some(due);
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.after(due.saturating_duration_since(now));
            }
        }
    });
}

fn response(world: &mut World, hub: &mut Hub, owners: &[Entity], key: &Key, event: lsp::Event) {
    match event {
        lsp::Event::Ready { formatting } => {
            if let Some(session) = hub.sessions.get_mut(key) {
                session.ready = true;
                session.formatting = formatting;
            }
            for owner in owners {
                if world.get::<Panel>(*owner).unwrap().connected.contains(key) {
                    message(
                        world,
                        *owner,
                        "Connected · Ctrl+Space completes · Ctrl+Shift+I formats",
                    );
                }
            }
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.after(Duration::from_millis(20));
            }
        }
        lsp::Event::Failed(error) => {
            hub.pending.retain(|_, (pending, _)| pending != key);
            if let Some(session) = hub.sessions.get_mut(key) {
                session.failed = true;
            }
            for owner in owners {
                if world.get::<Panel>(*owner).unwrap().connected.contains(key) {
                    message(world, *owner, &error);
                }
            }
        }
        lsp::Event::RequestError {
            token,
            message: error,
        } => {
            if let Some(ticket) = hub.pending.remove(&token).map(|(_, ticket)| ticket) {
                message(world, ticket.owner, error);
            }
        }
        lsp::Event::Completions {
            token,
            path,
            identity,
            revision,
            items,
        } => {
            let Some(ticket) = hub.pending.remove(&token).map(|(_, ticket)| ticket) else {
                return;
            };
            if ticket.path != path
                || ticket.identity != identity
                || ticket.revision != revision
                || !current(world, &ticket, true)
            {
                message(world, ticket.owner, "Completion expired; request it again");
                return;
            }
            let results = world.get::<Panel>(ticket.owner).unwrap().results;
            crate::sand_panel::clear(world, results);
            for (index, item) in items.iter().enumerate() {
                crate::sand_panel::button(
                    world,
                    results,
                    ticket.owner,
                    &item.label,
                    actions::Control::ChooseCompletion(index),
                );
            }
            message(
                world,
                ticket.owner,
                if items.is_empty() {
                    "No completions"
                } else {
                    "Choose a completion"
                },
            );
            let owner = ticket.owner;
            world.get_mut::<Panel>(owner).unwrap().completion = Some((ticket, items));
        }
        lsp::Event::Formatted {
            token,
            path,
            identity,
            revision,
            edits,
        } => {
            let Some(ticket) = hub.pending.remove(&token).map(|(_, ticket)| ticket) else {
                return;
            };
            if ticket.path != path || ticket.identity != identity || ticket.revision != revision {
                return;
            }
            if let Err(error) = apply(world, &ticket, &edits) {
                message(world, ticket.owner, error);
            }
        }
        lsp::Event::Diagnostics {
            path,
            identity,
            revision,
            items,
        } => {
            if world
                .resource::<Documents>()
                .0
                .get(&path)
                .is_none_or(|doc| {
                    doc.buffer.identity() != identity || doc.buffer.revision() != revision
                })
            {
                return;
            }
            hub.diagnostics.insert(
                (key.clone(), path.clone()),
                (identity, revision, items.clone()),
            );
            for owner in owners {
                if world
                    .get::<Ide>(*owner)
                    .is_none_or(|ide| ide.active.as_ref() != Some(&path))
                    || !world.get::<Panel>(*owner).unwrap().connected.contains(key)
                {
                    continue;
                }
                let mut panel = world.get_mut::<Panel>(*owner).unwrap();
                if panel.completion.is_some() {
                    continue;
                }
                panel.diagnostics = items.clone();
                panel.diagnostic_stamp = Some((path.clone(), identity, revision));
                let results = panel.results;
                crate::sand_panel::clear(world, results);
                for (index, item) in items.iter().enumerate() {
                    crate::sand_panel::button(
                        world,
                        results,
                        *owner,
                        &format!(
                            "{} · line {} · {}",
                            if item.severity == 1 {
                                "Error"
                            } else {
                                "Warning"
                            },
                            item.line + 1,
                            item.message
                        ),
                        actions::Control::Diagnostic(index),
                    );
                }
                message(
                    world,
                    *owner,
                    format!("Connected · {} diagnostics", items.len()),
                );
            }
        }
    }
}
