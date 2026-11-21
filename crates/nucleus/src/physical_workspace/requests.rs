use std::collections::BTreeSet;

use super::{
    MAX_REQUESTS, MAX_RUNNING, MIN_SELECTION_CONFIDENCE, Request, Runtime, Status, Target, text,
    valid_id,
};

impl Runtime {
    /// Adds a validated request to the workspace queue.
    ///
    /// # Errors
    ///
    /// Returns the first clock, surface, request, or capacity diagnostic.
    pub fn enqueue(&mut self, request: Request, workspace: u64, now_ms: u64) -> Result<(), String> {
        self.advance(now_ms)?;
        let surface = self.surface(&request.surface, workspace)?;
        if !valid_id(&request.id)
            || !valid_id(&request.user)
            || !request.target.valid()
            || !text(&request.prompt, 4096)
            || request.status != Status::Queued
            || request.created_ms != now_ms
            || request.updated_ms != now_ms
            || request.result.is_some()
            || request.calibration_revision != surface.calibration_revision
            || request.observations.len() > 8
            || self.requests.contains_key(&request.id)
            || request.observations.iter().collect::<BTreeSet<_>>().len()
                != request.observations.len()
            || request.observations.iter().any(|id| {
                !self.observations.get(id).is_some_and(|e| {
                    e.surface == request.surface
                        && !e.projected_content
                        && e.confidence >= MIN_SELECTION_CONFIDENCE
                })
            })
        {
            return Err("Invalid request, target or observation scope.".into());
        }
        if self.requests.len() >= MAX_REQUESTS {
            return Err("Request capacity reached; dismiss finished work first.".into());
        }
        self.requests.insert(request.id.clone(), request);
        Ok(())
    }

    /// Starts eligible requests in creation-time and identifier order.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the clock would move backwards.
    ///
    /// # Panics
    ///
    /// Panics if a queued request in the public map has no entry under its own identifier.
    #[expect(
        clippy::expect_used,
        reason = "preserves the public requests-map invariant"
    )]
    pub fn dispatch(&mut self, workspace: u64, now_ms: u64) -> Result<Vec<Request>, String> {
        self.advance(now_ms)?;
        let mut active = 0;
        let mut targets = BTreeSet::new();
        let mut queued = Vec::new();
        for request in self.requests.values() {
            match request.status {
                Status::Running | Status::Waiting => {
                    active += 1;
                    targets.insert((request.target.organ.clone(), request.target.record.clone()));
                }
                Status::Queued => queued.push((request.created_ms, request.id.clone())),
                _ => {}
            }
        }
        let mut available = MAX_RUNNING.saturating_sub(active);
        queued.sort();
        let surfaces: BTreeSet<_> = self
            .configuration
            .surfaces
            .iter()
            .filter(|s| s.workspace == workspace)
            .map(|surface| surface.id.as_str())
            .collect();
        let mut output = Vec::new();
        for (_, id) in queued {
            let request = self
                .requests
                .get_mut(&id)
                .expect("the queue was collected from the current requests");
            if !surfaces.contains(request.surface.as_str()) || available == 0 {
                continue;
            }
            let target = (request.target.organ.clone(), request.target.record.clone());
            if targets.contains(&target) {
                continue;
            }
            if request
                .observations
                .iter()
                .any(|id| !self.observations.contains_key(id))
            {
                request.status = Status::Canceled;
                continue;
            }
            request.status = Status::Running;
            request.updated_ms = now_ms;
            targets.insert(target);
            available -= 1;
            output.push(request.clone());
        }
        Ok(output)
    }

    /// Completes a request with a result from the authorized user and target.
    ///
    /// # Errors
    ///
    /// Returns the first clock, request, or result validation diagnostic.
    pub fn finish(
        &mut self,
        id: &str,
        target: &Target,
        user: &str,
        result: String,
        now_ms: u64,
    ) -> Result<(), String> {
        self.advance(now_ms)?;
        let request = self.requests.get_mut(id).ok_or("Unknown request.")?;
        let valid = matches!(request.status, Status::Running | Status::Waiting)
            && &request.target == target
            && request.user == user
            && text(&result, 8192)
            && self.configuration.surfaces.iter().any(|s| {
                s.id == request.surface && s.calibration_revision == request.calibration_revision
            });
        if !valid {
            if !request.terminal() {
                request.status = Status::Failed;
            }
            return Err("Result was canceled, stale, unauthorized or invalid.".into());
        }
        request.status = Status::Completed;
        request.updated_ms = now_ms;
        request.result = Some(result);
        Ok(())
    }

    /// Cancels a request that is still active or queued.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the request is unknown.
    pub fn cancel(&mut self, id: &str) -> Result<(), String> {
        let request = self.requests.get_mut(id).ok_or("Unknown request.")?;
        if !request.terminal() {
            request.status = Status::Canceled;
        }
        Ok(())
    }

    /// Cancels every request that has yet to finish.
    pub fn cancel_all(&mut self) {
        for request in self.requests.values_mut() {
            if !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    /// Discards a finished request.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the request is unknown or has yet to finish.
    pub fn dismiss(&mut self, id: &str) -> Result<(), String> {
        if !self.requests.get(id).is_some_and(Request::terminal) {
            return Err("Only finished work can be dismissed.".into());
        }
        self.requests.remove(id);
        Ok(())
    }
}

impl Request {
    /// Returns whether the request has reached its final status.
    pub fn terminal(&self) -> bool {
        matches!(
            self.status,
            Status::Completed | Status::Canceled | Status::Failed
        )
    }
}
