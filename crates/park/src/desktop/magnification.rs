use super::*;

impl Desktop {
    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn magnification_snapshot(&self) -> &lince_interface::host::magnification::Snapshot { &self.magnification.snapshot }

    pub(super) fn flush_magnification(&mut self) -> io::Result<()> {
        if self.magnification_sent.as_ref() == Some(&self.magnification.snapshot) { return Ok(()); }
        match self.worker.input(Input::Magnification(self.magnification.snapshot.clone())) {
            Ok(_) => self.magnification_sent = Some(self.magnification.snapshot.clone()),
            Err(std::sync::mpsc::TrySendError::Full(_)) => {}
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas is unavailable.")),
        }
        Ok(())
    }

    fn magnification_unavailable(&self, context: &lince_interface::host::control::Context) -> Option<&'static str> {
        if !self.focused || self.server.locked() { Some("Unlock and activate Park before changing display magnification.") }
        else if self.current.as_ref().and_then(|frame| frame.frame.input.control.context.as_ref()) != Some(context) {
            Some("The original Park view changed. Reload the preserved magnification draft before saving.")
        } else if !self.buttons.is_empty() || self.server.dragging() || self.manipulation.owns()
            || self.grabbed.as_ref().is_some_and(|target| !matches!(target, input::Target::Native)) {
            Some("Finish the active pointer interaction before changing display magnification.")
        } else { None }
    }

    pub(super) fn magnification_request(&mut self, request: lince_interface::host::magnification::Request) -> io::Result<()> {
        let path = self.command_file.clone();
        let unavailable = self.magnification_unavailable(&request.context);
        self.magnification.request(request, unavailable, |name, factor, expected| {
            let path = path.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable."))?;
            crate::commands::Commands::save_magnification(path, name, factor, expected)
        });
        self.flush_magnification()
    }

    pub(super) fn magnification_reload(&mut self, request: lince_interface::host::magnification::Reload) -> io::Result<()> {
        let path = self.command_file.clone();
        let unavailable = self.magnification_unavailable(&request.context);
        self.magnification.reload(request, unavailable, || {
            let path = path.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable."))?;
            let commands = crate::commands::Commands::load(path)?;
            Ok(commands.outputs.into_iter().map(|(name, output)| (name, output.magnification)).collect())
        });
        self.flush_magnification()
    }
}
