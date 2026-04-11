use {
    crate::{
        HttpServeMode,
        application::{
            ai_builder::AiBuilderState, backend_api::BackendApiService,
            kanban_actions::KanbanActionService, kanban_filters::KanbanFilterService,
            kanban_streams::KanbanStreamService, state::AppState,
            trail_widget::TrailWidgetService, widget_runtime::WidgetRuntimeService,
        },
        domain::board::{BoardCard, BoardState, BoardWorkspace},
        infrastructure::{
            auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
            organ_store::OrganStore, package_catalog_store::PackageCatalogStore,
            package_preview_store::PackagePreviewStore, terminal_store::TerminalSessionStore,
            widget_bridge_store::WidgetBridgeStore,
        },
        presentation::http::router::build_router,
    },
    injection::cross_cutting::dependency_injection,
    persistence::{
        connection::connection, seeder::seed, storage::StorageService,
        write_coordinator::spawn_write_coordinator,
    },
    sqlx::SqlitePool,
    std::{env, error::Error, net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc},
    tokio::{
        io::{AsyncBufReadExt, BufReader},
        process::Command as TokioCommand,
    },
    tokio::time::{Duration, sleep},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn browser_trail_open_keeps_done_and_children_visible() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _env_guard = TestEnvGuard::prepare()?;
    let db = Arc::new(connection().await?);
    sqlx::migrate!("../../migrations").run(&*db).await?;
    seed(&*db).await?;
    sqlx::query("UPDATE configuration SET bucket_enabled = 0 WHERE quantity = 1")
        .execute(&*db)
        .await?;

    let writer = spawn_write_coordinator().await?;
    let storage = Arc::new(StorageService::from_database(&*db).await?);
    let services = dependency_injection(db.clone(), storage, writer.clone());
    let auth = AppAuth::new();
    let board_state = BoardStateStore::new().map_err(std::io::Error::other)?;
    let organs = OrganStore::new(db.clone(), writer.clone());
    let backend = BackendApiService::new(services.clone(), Arc::new("trail-browser-test-secret".into()));
    let manas = ManasGateway::new()?;
    let package_catalog = PackageCatalogStore::new().map_err(std::io::Error::other)?;
    let app_state = AppState {
        ai: AiBuilderState::new(),
        auth: auth.clone(),
        backend: backend.clone(),
        board_state: board_state.clone(),
        local_auth_required: false,
        manas: manas.clone(),
        organs: organs.clone(),
        packages: package_catalog,
        package_previews: PackagePreviewStore::new(),
        terminal: TerminalSessionStore::new(),
        widget_bridge: WidgetBridgeStore::new(),
        kanban_actions: KanbanActionService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            false,
            manas.clone(),
            organs.clone(),
        ),
        kanban_filters: KanbanFilterService::new(board_state.clone()),
        kanban_streams: KanbanStreamService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            KanbanFilterService::new(board_state.clone()),
            false,
            manas.clone(),
            organs.clone(),
        ),
        trail_widget: TrailWidgetService::new(
            auth.clone(),
            backend.clone(),
            board_state.clone(),
            false,
            manas.clone(),
            organs.clone(),
        ),
        widget_runtime: WidgetRuntimeService::new(auth, board_state.clone(), false, organs),
    };

    let addr = free_local_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    let app = build_router(app_state.clone(), HttpServeMode::FullUi);
    let server_task = tokio::spawn(async move { axum::serve(listener, app).await });

    seed_browser_board(&board_state).await?;
    seed_browser_records(&*db).await?;
    wait_for_http_ready(local_addr).await?;

    if browser_watch_enabled() {
        eprintln!("browser-watch enabled: launching a headed Chromium window");
    }
    let chromium_output = run_chromium(local_addr).await?;

    server_task.abort();

    if chromium_output.trim() != "pass"
    {
        return Err(format!(
            "browser trail regression failed\nstdout:\n{}\nstderr:\n{}",
            chromium_output,
            ""
        )
        .into());
    }

    Ok(())
}

async fn seed_browser_board(board_state: &BoardStateStore) -> Result<(), Box<dyn Error + Send + Sync>> {
    let trail_package_html = String::new();

    let next_state = BoardState {
        density: 4,
        global_streams_enabled: true,
        active_workspace_id: "space-1".into(),
        workspaces: vec![BoardWorkspace {
            id: "space-1".into(),
            name: "Area 1".into(),
            cards: vec![
                BoardCard {
                    id: "trail-browser-widget".into(),
                    kind: "package".into(),
                    title: "Trail Relation".into(),
                    description: "Browser regression trail".into(),
                    text: String::new(),
                    html: trail_package_html,
                    author: "Lince Labs".into(),
                    permissions: vec![
                        "bridge_state".into(),
                        "read_view_stream".into(),
                        "write_records".into(),
                        "write_table".into(),
                    ],
                    package_name: "trail_relation.lince".into(),
                    requires_server: true,
                    server_id: "local-dev".into(),
                    view_id: None,
                    streams_enabled: true,
                    widget_state: serde_json::json!({}),
                    x: 1,
                    y: 1,
                    w: 7,
                    h: 6,
                },
            ],
        }],
    };

    board_state
        .replace(next_state)
        .await
        .map_err(std::io::Error::other)?;

    Ok(())
}

async fn seed_browser_records(db: &SqlitePool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let inserts = [
        (1_i64, 1_f64, "Alpha Root", "Root record that should stay Done"),
        (2_i64, 0_f64, "Alpha Child", "Child should appear when the root is done"),
        (30_i64, 1_f64, "Chain Root", "Root record used to test recursive reveal"),
        (31_i64, 0_f64, "Chain Child", "Child should become Ready after the root is done"),
        (32_i64, 0_f64, "Chain Grandchild", "Grandchild should become Ready after its parent is done"),
        (33_i64, 0_f64, "Chain Great-Grandchild", "This should become visible after the chain propagates"),
    ];

    for (id, quantity, head, body) in inserts {
        sqlx::query("INSERT OR REPLACE INTO record(id, quantity, head, body) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(quantity)
            .bind(head)
            .bind(body)
            .execute(db)
            .await?;
    }

    sqlx::query(
        "INSERT OR REPLACE INTO record_link(record_id, link_type, target_table, target_id) VALUES (?, 'parent', 'record', ?)",
    )
    .bind(2_i64)
    .bind(1_i64)
    .execute(db)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO record_link(record_id, link_type, target_table, target_id) VALUES (?, 'parent', 'record', ?)",
    )
    .bind(31_i64)
    .bind(30_i64)
    .execute(db)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO record_link(record_id, link_type, target_table, target_id) VALUES (?, 'parent', 'record', ?)",
    )
    .bind(32_i64)
    .bind(31_i64)
    .execute(db)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO record_link(record_id, link_type, target_table, target_id) VALUES (?, 'parent', 'record', ?)",
    )
    .bind(33_i64)
    .bind(32_i64)
    .execute(db)
    .await?;

    Ok(())
}

async fn wait_for_http_ready(addr: SocketAddr) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);

    loop {
        if let Ok(response) = client.get(&url).send().await
            && response.status().is_success()
        {
            return Ok(());
        }

        if tokio::time::Instant::now() > deadline {
            return Err(format!("server did not become ready at {url}").into());
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn run_chromium(addr: SocketAddr) -> Result<String, Box<dyn Error + Send + Sync>> {
    let browser_profile = temp_dir("chromium-profile");
    let board_url = format!("http://{addr}/");
    let watch_mode = browser_watch_enabled();
    let mut chromium = TokioCommand::new("chromium");
    if !watch_mode {
        chromium.arg("--headless=new");
    } else {
        chromium.arg("--auto-open-devtools-for-tabs");
    }
    let mut chromium = chromium
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .arg("--disable-background-timer-throttling")
        .arg("--disable-renderer-backgrounding")
        .arg("--run-all-compositor-stages-before-draw")
        .arg("--window-size=1920,1400")
        .arg("--remote-debugging-port=0")
        .arg("--user-data-dir")
        .arg(&browser_profile)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to launch chromium: {error}"))?;

    let stderr = chromium.stderr.take().ok_or_else(|| {
        std::io::Error::other("chromium stderr was not piped")
    })?;
    let mut stderr_lines = BufReader::new(stderr).lines();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut remote_debug_port = None;
    let mut chromium_stderr = String::new();

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), stderr_lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                chromium_stderr.push_str(&line);
                chromium_stderr.push('\n');
                if let Some(port) = parse_devtools_port(&line) {
                    remote_debug_port = Some(port);
                    break;
                }
            }
            Ok(Ok(None)) => {
                break;
            }
            Ok(Err(error)) => {
                let _ = chromium.kill().await;
                let _ = chromium.wait().await;
                return Err(Box::new(error));
            }
            Err(_) => {}
        }
    }

    let remote_debug_port = remote_debug_port.ok_or_else(|| {
        Box::<dyn Error + Send + Sync>::from(format!(
            "Chromium did not announce a DevTools websocket port\nstderr:\n{chromium_stderr}"
        ))
    })?;

    let result = run_chromium_cdp_test(remote_debug_port, &board_url, watch_mode).await;

    if watch_mode {
        sleep(Duration::from_secs(3)).await;
    }

    let _ = chromium.kill().await;
    let _ = chromium.wait().await;

    result
}

async fn run_chromium_cdp_test(
    port: u16,
    board_url: &str,
    watch_mode: bool,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let output = TokioCommand::new("node")
        .arg("-e")
        .arg(chromium_cdp_test_script())
        .arg(port.to_string())
        .arg(board_url)
        .arg(if watch_mode { "watch" } else { "default" })
        .output()
        .await
        .map_err(|error| format!("failed to launch node: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "browser CDP test failed (status: {:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_devtools_port(line: &str) -> Option<u16> {
    let marker = "DevTools listening on ws://127.0.0.1:";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('/')?;
    rest[..end].parse().ok()
}

fn chromium_cdp_test_script() -> &'static str {
    include_str!("browser_cdp_test.js")
}

fn browser_watch_enabled() -> bool {
    cfg!(feature = "browser-watch")
}

fn free_local_addr() -> Result<SocketAddr, Box<dyn Error + Send + Sync>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    Ok(addr)
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lince-trail-browser-{}-{label}",
        uuid::Uuid::new_v4()
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

struct TestEnvGuard {
    _root: PathBuf,
}

impl TestEnvGuard {
    fn prepare() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let root = temp_dir("xdg");
        unsafe {
            env::set_var("XDG_CONFIG_HOME", &root);
        }
        Ok(Self { _root: root })
    }
}
