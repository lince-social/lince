use {
    crate::{
        domain::board::{BoardState, default_board_state},
        infrastructure::paths,
    },
    std::{path::PathBuf, sync::Arc},
    tokio::sync::RwLock,
};

#[derive(Clone)]
pub struct BoardStateStore {
    path: Arc<PathBuf>,
    state: Arc<RwLock<BoardState>>,
}

impl BoardStateStore {
    pub fn new() -> Result<Self, String> {
        let path = paths::board_state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Nao consegui criar a pasta do board salvo: {error}"))?;
        }
        let state = load_state_from_disk(&path).unwrap_or_else(|error| {
            tracing::warn!("board state load failed, using default: {error}");
            default_board_state()
        });

        Ok(Self {
            path: Arc::new(path),
            state: Arc::new(RwLock::new(state)),
        })
    }

    pub async fn snapshot(&self) -> BoardState {
        self.state.read().await.clone()
    }

    pub async fn replace(&self, next_state: BoardState) -> Result<BoardState, String> {
        persist_state_to_disk(&self.path, &next_state)?;
        let mut state = self.state.write().await;
        *state = next_state.clone();
        Ok(next_state)
    }
}

fn load_state_from_disk(path: &PathBuf) -> Result<BoardState, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<BoardState>(&raw)
            .map_err(|error| format!("Nao consegui interpretar o board salvo: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_board_state()),
        Err(error) => Err(format!("Nao consegui ler o board salvo: {error}")),
    }
}

fn persist_state_to_disk(path: &PathBuf, state: &BoardState) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(state)
        .map_err(|error| format!("Nao consegui serializar o board: {error}"))?;
    let tmp_path = path.with_extension("json.tmp");

    std::fs::write(&tmp_path, raw)
        .map_err(|error| format!("Nao consegui escrever o board no disco: {error}"))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|error| format!("Nao consegui finalizar o board salvo: {error}"))?;

    Ok(())
}
