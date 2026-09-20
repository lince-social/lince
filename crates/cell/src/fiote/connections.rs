use super::*;

pub(super) struct Opened {
    pub record: String,
    pub connection: transport::mcp::Connection,
}

impl Host {
    pub(super) async fn tool_connections(
        &self,
        record: &str,
    ) -> Vec<fiote::config::ToolConnection> {
        let mut connections = self.connections.lock().await;
        connections.retain(|_, open| !open.connection.is_closed());
        connections
            .iter()
            .filter(|(_, open)| open.record == record)
            .map(|(thread, open)| fiote::config::ToolConnection {
                thread: thread.clone(),
                url: open.connection.url.clone(),
                token: open.connection.token.clone(),
            })
            .collect()
    }

    pub(super) async fn open_tools(&self, record: &str, thread: &str) -> Result<(), String> {
        self.engine
            .access_scope(true, async {
                self.open_tools_inner(record, thread)
                    .await
                    .map_err(engine::EngineError::Consequence)
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn open_tools_inner(&self, record: &str, thread: &str) -> Result<(), String> {
        if !self
            .record_threads(record)
            .await?
            .iter()
            .any(|row| row.uid == thread)
            && self.conversation_fiote(record, thread).await?.as_deref() != Some(record)
        {
            return Err("This thread does not belong to this Fiote Record.".into());
        }
        self.prepare(record).await?;
        let mut connections = self.connections.lock().await;
        connections.retain(|_, open| !open.connection.is_closed());
        if connections
            .get(thread)
            .is_some_and(|open| open.record == record)
        {
            return Ok(());
        }
        if connections.len() >= 8 {
            return Err(
                "Close an agent tool connection before opening another; eight are already open."
                    .into(),
            );
        }
        let context = self
            .engine
            .access_scope(true, async {
                self.record(record)
                    .await
                    .map_err(engine::EngineError::Consequence)?;
                let thread_row = self
                    .record(thread)
                    .await
                    .map_err(engine::EngineError::Consequence)?;
                if thread_row.kind != RecordKind::Thread.as_str() {
                    return Err(engine::EngineError::Consequence(
                        "Choose a thread for the agent connection.".into(),
                    ));
                }
                let predicate = store::concepts::resolve(&self.engine.store.pool, "thread-of")
                    .await?
                    .ok_or_else(|| {
                        engine::EngineError::Consequence(
                            "This thread is not linked to a Record.".into(),
                        )
                    })?;
                let parents = store::assertions::objects_from_subject(
                    &self.engine.store.pool,
                    thread,
                    &predicate,
                )
                .await?;
                if !parents.iter().any(|parent| parent.uid == record)
                    && self
                        .conversation_fiote(record, thread)
                        .await
                        .map_err(engine::EngineError::Consequence)?
                        .as_deref()
                        != Some(record)
                {
                    return Err(engine::EngineError::Consequence(
                        "This thread does not belong to this Fiote Record.".into(),
                    ));
                }
                let author = record.to_string();
                if self
                    .load(record)
                    .map_err(engine::EngineError::Consequence)?
                    .is_none()
                {
                    save(
                        &self
                            .path(record)
                            .map_err(engine::EngineError::Consequence)?,
                        &Configuration {
                            author: author.clone(),
                            settings: Settings::default(),
                            agent: None,
                        },
                    )
                    .map_err(engine::EngineError::Consequence)?;
                }
                if self
                    .record(&author)
                    .await
                    .map_err(engine::EngineError::Consequence)?
                    .kind
                    != RecordKind::Person.as_str()
                {
                    return Err(engine::EngineError::Consequence(
                        "The agent author is not a Person Record.".into(),
                    ));
                }
                Ok(transport::native::Context {
                    agent: author,
                    record: record.into(),
                    thread: thread.into(),
                })
            })
            .await
            .map_err(|error| error.to_string())?;
        let instructions = self.session_instructions(record, thread).await?;
        let native = transport::Session::local(
            self.engine.clone(),
            Arc::new(transport::LaneHub::new()),
            nucleus::new_uid("agent-tools"),
        )
        .into_native_tools(context)
        .with_instructions(instructions);
        let connection = transport::mcp::Connection::open(native).await?;
        connections.insert(
            thread.into(),
            Opened {
                record: record.into(),
                connection,
            },
        );
        Ok(())
    }
}
