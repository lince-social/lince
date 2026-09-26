use super::*;

pub(in crate::fiote) async fn start(agents: &Agents, record: &str) -> Result<String, String> {
    let mut state = agents.info.lock().await;
    let info = state
        .get_mut(record)
        .ok_or("Discover the agent before signing in.")?;
    if info["loginPending"] == true {
        return Err("Finish or cancel the current sign-in first.".into());
    }
    let id = nucleus::new_uid("login");
    info["loginId"] = id.clone().into();
    info["loginPending"] = true.into();
    info.as_object_mut().unwrap().remove("loginResult");
    info.as_object_mut().unwrap().remove("connectionCheck");
    Ok(id)
}

pub(in crate::fiote) async fn finish(
    state: Arc<Mutex<HashMap<String, Value>>>,
    record: String,
    id: String,
    result: Result<(), String>,
) {
    if let Some(info) = state.lock().await.get_mut(&record) {
        if info["loginId"] != id {
            return;
        }
        info["loginPending"] = false.into();
        info["loginResult"] = match result {
            Ok(()) => "Sign-in completed. Check connection to confirm session access without using model tokens.".into(),
            Err(error) => format!("Sign-in failed: {error}").into(),
        };
        info.as_object_mut().unwrap().remove("loginId");
    }
}

impl Host {
    pub(super) async fn start_terminal_login(
        &self,
        record: &str,
        config: acp::Config,
        method: acp::AuthMethodTerminal,
    ) -> Result<(), String> {
        let id = start(&self.agents, record).await?;
        let terminal = match tokio::task::spawn_blocking(move || {
            acp::terminal::LoginTerminal::start(&config, &method)
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result)
        {
            Ok(terminal) => terminal,
            Err(error) => {
                finish(
                    self.agents.info.clone(),
                    record.into(),
                    id,
                    Err(error.clone()),
                )
                .await;
                return Err(error);
            }
        };
        if let Some((_, old)) = self
            .agents
            .terminals
            .lock()
            .await
            .insert(record.into(), (id.clone(), terminal.clone()))
        {
            old.close();
        }
        if let Some(info) = self.agents.info.lock().await.get_mut(record) {
            info["terminalLogin"] = id.clone().into();
        }
        let state = self.agents.info.clone();
        let record = record.to_string();
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
            let result = loop {
                if state
                    .lock()
                    .await
                    .get(&record)
                    .is_none_or(|info| info["loginId"] != id)
                {
                    terminal.close();
                    return;
                }
                if let Some(result) = terminal.result() {
                    break result;
                }
                if tokio::time::Instant::now() >= deadline {
                    terminal.close();
                    break Err("Terminal login timed out.".into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            };
            finish(state, record, id, result).await;
        });
        Ok(())
    }

    pub(in crate::fiote) async fn login_terminal(
        &self,
        request: acp::terminal::TerminalRequest,
    ) -> Result<acp::terminal::TerminalFrame, String> {
        self.record(&request.record).await?;
        let mut terminals = self.agents.terminals.lock().await;
        let (id, terminal) = terminals
            .get(&request.record)
            .ok_or("This terminal login has ended.")?;
        if id != &request.login {
            return Err("This request belongs to an earlier login.".into());
        }
        let terminal = terminal.clone();
        if request.close {
            terminals.remove(&request.record);
        }
        drop(terminals);
        tokio::task::spawn_blocking(move || terminal.exchange(&request))
            .await
            .map_err(|error| error.to_string())?
    }

    pub(in crate::fiote) async fn logout_agent(&self, record: &str) -> Result<(), String> {
        self.record(record).await?;
        let running = self.running.lock().await;
        if running.values().any(|run| run.record == record) {
            return Err("Stop this Fiote's active turns before signing out.".into());
        }
        if self
            .agents
            .info
            .lock()
            .await
            .get(record)
            .is_some_and(|info| info["loginPending"] == true)
        {
            return Err("Finish or cancel sign-in before signing out.".into());
        }
        let connection = {
            let discovery = self.agents.discovery.lock().await;
            discovery
                .get(&format!("login:{record}"))
                .or_else(|| discovery.get(record))
                .map(|(_, connection)| connection.clone())
                .ok_or("Connect the agent before signing out.")?
        };
        connection.logout().await?;
        self.agents.close_record(record).await;
        let mut discovery = self.agents.discovery.lock().await;
        for key in [record.to_string(), format!("login:{record}")] {
            if let Some((_, connection)) = discovery.remove(&key) {
                connection.close();
            }
        }
        if let Some(info) = self.agents.info.lock().await.get_mut(record) {
            info["loginResult"] =
                "Agent logout completed. Provider-side credential revocation is not confirmed."
                    .into();
            info.as_object_mut().unwrap().remove("connectionCheck");
        }
        Ok(())
    }
    pub(in crate::fiote) async fn cancel_agent_login(&self, record: &str) -> Result<(), String> {
        self.record(record).await?;
        if let Some((_, terminal)) = self.agents.terminals.lock().await.remove(record) {
            terminal.close();
        }
        if let Some(info) = self.agents.info.lock().await.get_mut(record) {
            info.as_object_mut().unwrap().remove("terminalLogin");
            info["loginPending"] = false.into();
            info["loginResult"] =
                "Sign-in cancelled. Choose Change provider / sign in to retry.".into();
            info.as_object_mut().unwrap().remove("loginId");
            info.as_object_mut().unwrap().remove("connectionCheck");
        }
        self.agents.clear_options(record).await;
        let mut discovery = self.agents.discovery.lock().await;
        for key in [record.to_string(), format!("login:{record}")] {
            if let Some((_, connection)) = discovery.remove(&key) {
                connection.close();
            }
        }
        Ok(())
    }
}
