use super::*;
use cell::fiote_connection::{Connection, Profile, Request};

#[derive(Clone, Copy, PartialEq)]
enum Route {
    Harness,
    Model,
    External,
}
#[derive(Component)]
struct Form {
    route: Route,
    fields: Vec<Entity>,
    credential: Entity,
    password: Entity,
    original: Option<Profile>,
}

pub(super) fn editing(world: &World, owner: Entity) -> bool {
    world.get::<Form>(owner).is_some()
}
#[derive(Clone)]
pub(super) struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        world.entity_mut(owner).remove::<Form>();
        super::show(world, owner, Step::Connections);
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(
            world,
            owner,
            FioteRequest::Connections {
                record,
                request: Request::Inspect,
            },
        );
    }
}

pub(super) fn show(world: &mut World, owner: Entity, content: Entity, saved: Option<&FioteStatus>) {
    world.entity_mut(owner).remove::<Form>();
    crate::edit_mode::label(world, content, "Your AI connections", 20.0);
    crate::edit_mode::label(
        world,
        content,
        "Choose your own compatible harness, a supported local/API model, or tools for an external AI client. Check does not send a prompt.",
        13.0,
    );
    for (label, route) in [
        ("New harness connection", Route::Harness),
        ("New local/API model", Route::Model),
        ("External AI tools", Route::External),
    ] {
        crate::description::button(
            world,
            content,
            owner,
            label,
            Edit {
                route,
                profile: None,
            },
        );
    }
    crate::description::button(world, content, owner, "Model login options", ModelLogin);
    crate::description::button(
        world,
        content,
        owner,
        "Disconnect selected connection",
        Control(Request::Deselect),
    );
    crate::description::button(world, content, owner, "Refresh", Control(Request::Inspect));
    crate::description::button(
        world,
        content,
        owner,
        "Discover installed AI",
        Control(Request::Discover {
            refresh_registry: false,
        }),
    );
    crate::description::button(
        world,
        content,
        owner,
        "Refresh ACP registry",
        Control(Request::Discover {
            refresh_registry: true,
        }),
    );
    let Some(saved) = saved else { return };
    if let Some(discovery) = &saved.connections.discovery {
        crate::edit_mode::label(world, content, &discovery.detail, 13.0);
        for candidate in discovery.candidates.iter().filter(|entry| {
            entry.availability != cell::fiote_communication::discovery::Availability::Unavailable
        }) {
            crate::edit_mode::label(world, content, &candidate.name, 16.0);
            crate::edit_mode::label(world, content, &candidate.detail, 13.0);
            if let Some(config) = &candidate.config {
                crate::description::button(
                    world,
                    content,
                    owner,
                    &format!("Use {}", candidate.name),
                    Edit {
                        route: Route::Harness,
                        profile: Some(Profile {
                            id: nucleus::new_uid("connection"),
                            name: candidate.name.clone(),
                            connection: Connection::Harness {
                                config: config.clone(),
                            },
                        }),
                    },
                );
            } else if let Some(installation) = &candidate.installation {
                crate::edit_mode::label(world, content, installation, 13.0);
            }
        }
    }
    if saved.agent.is_some() {
        crate::description::button(
            world,
            content,
            owner,
            "Session settings and authentication",
            management::Connection,
        );
    }
    for profile in &saved.connections.profiles.entries {
        crate::edit_mode::label(
            world,
            content,
            &format!(
                "{}{}",
                profile.name,
                if saved.connections.profiles.selected.as_deref() == Some(&profile.id) {
                    " · selected"
                } else {
                    ""
                }
            ),
            16.0,
        );
        let row = crate::sand_panel::row(world, content);
        for (label, command) in [
            (
                "Check",
                Request::Check {
                    id: profile.id.clone(),
                },
            ),
            (
                "Connect",
                Request::Select {
                    id: profile.id.clone(),
                    api_key: None,
                    password: None,
                },
            ),
            (
                "Remove",
                Request::Remove {
                    id: profile.id.clone(),
                },
            ),
        ] {
            crate::description::button(world, row, owner, label, Control(command));
        }
        let route = match profile.connection {
            Connection::Harness { .. } => Route::Harness,
            Connection::Model { .. } => Route::Model,
            Connection::External => Route::External,
        };
        crate::description::button(
            world,
            row,
            owner,
            "Edit / credentials",
            Edit {
                route,
                profile: Some(profile.clone()),
            },
        );
    }
    if let Some(check) = &saved.connections.check {
        crate::edit_mode::label(
            world,
            content,
            &format!(
                "{} · {:?}: {}\n{}",
                if check.ready {
                    "Ready"
                } else {
                    "Needs attention"
                },
                check.stage,
                check.detail,
                serde_json::to_string_pretty(&check.capabilities).unwrap()
            ),
            13.0,
        );
    }
    if saved
        .connections
        .profiles
        .selected
        .as_deref()
        .is_some_and(|id| {
            saved.connections.profiles.entries.iter().any(|profile| {
                profile.id == id && matches!(profile.connection, Connection::External)
            })
        })
    {
        crate::edit_mode::label(
            world,
            content,
            "Open a conversation’s Tools controls to copy its scoped MCP configuration. Your external client owns the conversation. MCP alone cannot receive Fiote wakeups. Disconnect those tools to revoke access.",
            13.0,
        );
    }
}

#[derive(Clone)]
struct ModelLogin;
impl Action for ModelLogin {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world
            .get::<Panel>(owner)
            .is_some_and(|panel| panel.pending.is_none())
        {
            world.entity_mut(owner).remove::<Form>();
            super::show(world, owner, Step::Providers);
        }
    }
}

#[derive(Clone)]
struct Control(Request);
impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world
            .get::<Panel>(owner)
            .is_some_and(|panel| panel.pending.is_some())
        {
            return;
        }
        let record = world.get::<Panel>(owner).unwrap().binding.uid.clone();
        request(
            world,
            owner,
            FioteRequest::Connections {
                record,
                request: self.0.clone(),
            },
        );
    }
}
#[derive(Clone)]
struct Edit {
    route: Route,
    profile: Option<Profile>,
}
impl Action for Edit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let content = world.get::<Panel>(owner).unwrap().content;
        world.entity_mut(content).despawn_children();
        let profile = self.profile.clone();
        let mut fields = Vec::new();
        let mut add = |world: &mut World, title: &str, initial: String| {
            let entity = field(world, content, title, false);
            world
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(&initial);
            fields.push(entity);
        };
        add(
            world,
            "Connection ID",
            profile
                .as_ref()
                .map(|profile| profile.id.clone())
                .unwrap_or_else(|| nucleus::new_uid("connection")),
        );
        add(
            world,
            "Name",
            profile
                .as_ref()
                .map(|profile| profile.name.clone())
                .unwrap_or_default(),
        );
        match self.route {
            Route::Harness => {
                let config = profile.as_ref().and_then(|profile| {
                    if let Connection::Harness { config } = &profile.connection {
                        Some(config)
                    } else {
                        None
                    }
                });
                crate::edit_mode::label(
                    world,
                    content,
                    "Use an ACP-compatible executable or an existing protocol bridge. The harness owns its login and model. Lince does not parse arbitrary CLI output.",
                    13.0,
                );
                add(
                    world,
                    "Executable",
                    config
                        .map(|config| config.command.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                add(
                    world,
                    "Arguments (JSON list)",
                    config
                        .map(|config| serde_json::to_string(&config.args).unwrap())
                        .unwrap_or_else(|| "[]".into()),
                );
                add(
                    world,
                    "Working directory",
                    config
                        .map(|config| config.directory.to_string_lossy().into_owned())
                        .unwrap_or_else(|| {
                            std::env::current_dir()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        }),
                );
                add(
                    world,
                    "Environment (JSON object)",
                    config
                        .map(|config| serde_json::to_string(&config.environment).unwrap())
                        .unwrap_or_else(|| "{}".into()),
                );
            }
            Route::Model => {
                let settings = profile.as_ref().and_then(|profile| {
                    if let Connection::Model { settings } = &profile.connection {
                        Some(settings)
                    } else {
                        None
                    }
                });
                if let Some(saved) = &world.get::<Panel>(owner).unwrap().saved {
                    crate::edit_mode::label(
                        world,
                        content,
                        &format!(
                            "Supported API shapes: {}",
                            saved
                                .providers
                                .iter()
                                .map(|provider| format!("{} ({})", provider.label, provider.id.0))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        13.0,
                    );
                }
                add(
                    world,
                    "API shape",
                    settings
                        .map(|settings| settings.provider.0.clone())
                        .unwrap_or_default(),
                );
                add(
                    world,
                    "Authentication method",
                    settings
                        .map(|settings| settings.auth_method.clone())
                        .unwrap_or_default(),
                );
                add(
                    world,
                    "Endpoint URL (blank uses library default)",
                    settings
                        .map(|settings| settings.endpoint.clone())
                        .unwrap_or_default(),
                );
                add(
                    world,
                    "Model",
                    settings
                        .map(|settings| settings.model.clone())
                        .unwrap_or_default(),
                );
                add(
                    world,
                    "Working directory",
                    settings
                        .map(|settings| settings.directory.to_string_lossy().into_owned())
                        .unwrap_or_else(|| {
                            std::env::current_dir()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        }),
                );
            }
            Route::External => {
                crate::edit_mode::label(
                    world,
                    content,
                    "Standard MCP tools for your own AI client. Create a scoped connection in the conversation’s Tools controls. No in-Lince chat or automatic activation is implied.",
                    13.0,
                );
            }
        }
        let credential = field(world, content, "API key (optional; saved in vault)", true);
        let password = field(world, content, "Vault password (when needed)", true);
        world.entity_mut(owner).insert(Form {
            route: self.route,
            fields,
            credential,
            password,
            original: profile,
        });
        if self.route == Route::Model {
            let providers = world
                .get::<Panel>(owner)
                .and_then(|panel| panel.saved.as_ref())
                .map(|saved| saved.providers.clone())
                .unwrap_or_default();
            for provider in providers {
                for method in provider.auth_methods {
                    if matches!(
                        method.kind,
                        cell::FioteAuthKind::ApiKey | cell::FioteAuthKind::None
                    ) {
                        crate::description::button(
                            world,
                            content,
                            owner,
                            &format!("{} · {}", provider.label, method.label),
                            UseProvider {
                                provider: provider.id.0.clone(),
                                method: method.id,
                                endpoint: provider.endpoint.clone(),
                            },
                        );
                    }
                }
            }
        }
        crate::description::button(world, content, owner, "Save profile", Save);
        crate::description::button(world, content, owner, "Check saved profile", Check);
        crate::description::button(
            world,
            content,
            owner,
            "Connect saved profile / unlock",
            Connect,
        );
        crate::description::button(world, content, owner, "Back to connections", Open);
    }
}

fn profile(world: &World, owner: Entity) -> Result<Profile, String> {
    let form = world
        .get::<Form>(owner)
        .ok_or("Connection editor is closed.")?;
    let text = |index| value(world, form.fields[index]);
    let connection = match form.route {
        Route::Harness => {
            let mut config = form
                .original
                .as_ref()
                .and_then(|profile| {
                    if let Connection::Harness { config } = &profile.connection {
                        Some(config.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| cell::FioteAgentConfig {
                    command: Default::default(),
                    args: Vec::new(),
                    directory: Default::default(),
                    environment: Default::default(),
                    additional_directories: Vec::new(),
                    session_meta: Default::default(),
                    options: Default::default(),
                    require_vault: false,
                });
            config.command = text(2)?.into();
            config.args = serde_json::from_str(&text(3)?)
                .map_err(|_| "Arguments must be a JSON list of strings.")?;
            config.directory = text(4)?.into();
            config.environment = serde_json::from_str(&text(5)?)
                .map_err(|_| "Environment must be a JSON object of strings.")?;
            Connection::Harness { config }
        }
        Route::Model => {
            let mut settings = FioteSettings::default();
            settings.enabled = true;
            settings.provider.0 = text(2)?;
            settings.auth_method = text(3)?;
            settings.endpoint = text(4)?;
            settings.model = text(5)?;
            settings.directory = text(6)?.into();
            Connection::Model { settings }
        }
        Route::External => Connection::External,
    };
    let mut profile = Profile {
        id: text(0)?,
        name: text(1)?,
        connection,
    };
    profile.validate()?;
    Ok(profile)
}
#[derive(Clone)]
struct Save;
impl Action for Save {
    fn apply(&self, world: &mut World, owner: Entity) {
        match profile(world, owner) {
            Ok(profile) => Control(Request::Save { profile }).apply(world, owner),
            Err(error) => {
                let status = world.get::<Panel>(owner).unwrap().status;
                crate::sand_panel::status(world, status, error);
            }
        }
    }
}
#[derive(Clone)]
struct Connect;
impl Action for Connect {
    fn apply(&self, world: &mut World, owner: Entity) {
        let result = (|| {
            let form = world
                .get::<Form>(owner)
                .ok_or("Connection editor is closed.")?;
            let key = value(world, form.credential)?;
            let password = value(world, form.password)?;
            Ok::<_, String>(Request::Select {
                id: value(world, form.fields[0])?,
                api_key: (!key.is_empty()).then_some(FioteSecret(key)),
                password: (!password.is_empty()).then_some(FioteSecret(password)),
            })
        })();
        match result {
            Ok(command) => {
                Control(command).apply(world, owner);
                if world
                    .get::<Panel>(owner)
                    .is_some_and(|panel| panel.pending.is_some())
                    && let Some(form) = world.get::<Form>(owner)
                {
                    for field in [form.credential, form.password] {
                        world
                            .get_mut::<EditableText>(field)
                            .unwrap()
                            .editor
                            .set_text("");
                    }
                }
            }
            Err(error) => {
                let status = world.get::<Panel>(owner).unwrap().status;
                crate::sand_panel::status(world, status, error);
            }
        }
    }
}

#[derive(Clone)]
struct UseProvider {
    provider: String,
    method: String,
    endpoint: String,
}
impl Action for UseProvider {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(form) = world.get::<Form>(owner) else {
            return;
        };
        if form.route != Route::Model {
            return;
        }
        let fields = form.fields.clone();
        for (index, value) in [(2, &self.provider), (3, &self.method), (4, &self.endpoint)] {
            world
                .get_mut::<EditableText>(fields[index])
                .unwrap()
                .editor
                .set_text(value);
        }
    }
}
#[derive(Clone)]
struct Check;
impl Action for Check {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(form) = world.get::<Form>(owner) {
            if let Ok(id) = value(world, form.fields[0]) {
                Control(Request::Check { id }).apply(world, owner);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::tests::deliver;

    fn click(world: &mut World, label: &str) {
        let button = world
            .query::<(
                &crate::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(world)
            .find(|(_, node)| node.label() == Some(label))
            .map(|(button, _)| button.clone())
            .unwrap_or_else(|| panic!("Missing UI control {label}"));
        button.actions.run(world, button.target);
    }

    #[tokio::test]
    async fn native_discovery_and_profile_controls_save_check_and_remove_through_cell() {
        let directory = tempfile::tempdir().unwrap();
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let record = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Fiote".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let host = std::sync::Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
                .await
                .unwrap(),
        );
        let runtime = cell::CellRuntime {
            speech: None,
            commands: Default::default(),
            store: engine.store.clone(),
            engine,
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            fiote: Some(host.clone()),
        };
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_plugins(Plugin);
        app.world_mut().insert_non_send(crate::cell_bridge::connect(
            runtime,
            crate::wake::WakeSignal::new(|| {}),
        ));
        let root = app.world_mut().spawn_empty().id();
        populate(
            app.world_mut(),
            root,
            RecordBinding {
                area: root,
                uid: record.clone(),
                source: Source::Local,
            },
        );
        deliver(&mut app).await;
        let owner = app
            .world_mut()
            .query_filtered::<Entity, With<Panel>>()
            .single(app.world())
            .unwrap();
        connections::Open.apply(app.world_mut(), owner);
        deliver(&mut app).await;
        click(app.world_mut(), "Discover installed AI");
        deliver(&mut app).await;
        assert!(
            app.world()
                .get::<Panel>(owner)
                .unwrap()
                .saved
                .as_ref()
                .unwrap()
                .connections
                .discovery
                .is_some()
        );
        click(app.world_mut(), "New harness connection");
        let fields = app.world().get::<Form>(owner).unwrap().fields.clone();
        assert_eq!(fields.len(), 6);
        let values = [
            "custom-harness".to_string(),
            "My own AI".to_string(),
            directory.path().join("missing-agent").display().to_string(),
            "[]".into(),
            directory.path().display().to_string(),
            "{}".into(),
        ];
        for (field, value) in fields.into_iter().zip(values) {
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(&value);
        }
        click(app.world_mut(), "Save profile");
        deliver(&mut app).await;
        assert!(
            app.world()
                .get::<Panel>(owner)
                .unwrap()
                .saved
                .as_ref()
                .unwrap()
                .connections
                .profiles
                .entries
                .iter()
                .any(|profile| profile.id == "custom-harness")
        );
        click(app.world_mut(), "Check saved profile");
        deliver(&mut app).await;
        let check = app
            .world()
            .get::<Panel>(owner)
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .connections
            .check
            .as_ref()
            .unwrap();
        assert!(!check.ready);
        assert_eq!(
            check.stage,
            cell::fiote_communication::check::Stage::Executable
        );
        click(app.world_mut(), "Back to connections");
        deliver(&mut app).await;
        click(app.world_mut(), "Remove");
        deliver(&mut app).await;
        assert!(
            app.world()
                .get::<Panel>(owner)
                .unwrap()
                .saved
                .as_ref()
                .unwrap()
                .connections
                .profiles
                .entries
                .is_empty()
        );
        host.stop_all().await;
    }

    fn form(world: &mut World, route: Route, values: &[&str]) -> Entity {
        let fields = values
            .iter()
            .map(|value| world.spawn(EditableText::new(value)).id())
            .collect();
        let credential = world.spawn(EditableText::new("api-secret")).id();
        let password = world.spawn(EditableText::new("vault-secret")).id();
        world
            .spawn(Form {
                route,
                fields,
                credential,
                password,
                original: None,
            })
            .id()
    }

    #[test]
    fn profile_forms_use_generic_routes_and_keep_secrets_out_of_profiles() {
        let mut world = World::new();
        let harness = form(
            &mut world,
            Route::Harness,
            &["harness", "My harness", "acp-fixture", "[]", "/tmp", "{}"],
        );
        let harness_profile = profile(&world, harness).unwrap();
        assert!(matches!(
            harness_profile.connection,
            Connection::Harness { .. }
        ));
        assert!(
            !serde_json::to_string(&harness_profile)
                .unwrap()
                .contains("secret")
        );
        let model = form(
            &mut world,
            Route::Model,
            &[
                "local",
                "My model",
                "Ollama",
                "none",
                "http://127.0.0.1:11434",
                "chosen-model",
                "/tmp",
            ],
        );
        let model_profile = profile(&world, model).unwrap();
        assert!(matches!(model_profile.connection, Connection::Model { .. }));
        assert!(
            !serde_json::to_string(&model_profile)
                .unwrap()
                .contains("secret")
        );
        let external = form(&mut world, Route::External, &["external", "My AI client"]);
        assert!(matches!(
            profile(&world, external).unwrap().connection,
            Connection::External
        ));
    }

    #[test]
    fn malformed_profile_fields_remain_editable_and_provider_choices_fill_supported_values() {
        let mut world = World::new();
        let owner = form(
            &mut world,
            Route::Harness,
            &[
                "harness",
                "My harness",
                "acp-fixture",
                "not a list",
                "/tmp",
                "{}",
            ],
        );
        assert!(profile(&world, owner).is_err());
        let field = world.get::<Form>(owner).unwrap().fields[3];
        assert_eq!(value(&world, field).unwrap(), "not a list");
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("[]");
        assert!(profile(&world, owner).is_ok());
        let owner = form(
            &mut world,
            Route::Model,
            &["model", "Model", "", "", "", "chosen", "/tmp"],
        );
        UseProvider {
            provider: "OpenAI".into(),
            method: "api-key".into(),
            endpoint: "https://example.test/v1".into(),
        }
        .apply(&mut world, owner);
        let profile = profile(&world, owner).unwrap();
        let Connection::Model { settings } = profile.connection else {
            panic!("model");
        };
        assert_eq!(settings.provider.0, "OpenAI");
        assert_eq!(settings.model, "chosen");
        assert_eq!(settings.endpoint, "https://example.test/v1/");
    }
}

#[cfg(test)]
mod polling_tests {
    use super::*;

    #[test]
    fn connection_editor_survives_first_status_and_preserves_focus_and_fields() {
        let mut app = App::new();
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_plugins(super::super::Plugin);
        let owner = app.world_mut().spawn(Node::default()).id();
        let content = app
            .world_mut()
            .spawn((Node::default(), ChildOf(owner)))
            .id();
        let status = app.world_mut().spawn(Text::default()).id();
        let record = nucleus::new_uid("r");
        app.world_mut().entity_mut(owner).insert(Panel {
            view_uid: record.clone(),
            pending_command: None,
            binding: RecordBinding {
                area: owner,
                uid: record.clone(),
                source: Source::Local,
            },
            status,
            content,
            step: Step::Connections,
            provider: None,
            method: None,
            labels: Vec::new(),
            selection: 0,
            choices: Vec::new(),
            fields: Vec::new(),
            pending: None,
            saved: None,
            poll: std::time::Instant::now(),
            automatic: true,
        });
        Edit {
            route: Route::Model,
            profile: None,
        }
        .apply(app.world_mut(), owner);
        let input = app.world().get::<Form>(owner).unwrap().fields[4];
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("http://127.0.0.1:9000/v1");
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Pressed);
        super::super::apply_status(
            app.world_mut(),
            owner,
            super::super::tests::fixture_status(&record),
        );
        assert!(editing(app.world(), owner));
        assert_eq!(
            value(app.world(), input).unwrap(),
            "http://127.0.0.1:9000/v1"
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(input));
    }
}
