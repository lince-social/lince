use super::{
    Commit, Evidence, Interaction, MAX_AGE_MS, MAX_OBSERVATIONS, MIN_SELECTION_CONFIDENCE, Phase,
    Pointer, Runtime, Status, Surface, valid_id,
};

const POINTER_TIMEOUT_MS: u64 = 500;
const REQUEST_TIMEOUT_MS: u64 = 120_000;
const MAX_POINTERS: usize = 16;

impl Runtime {
    /// Advances the adapter clock and expires stale state.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the clock would move backwards.
    pub fn advance(&mut self, now_ms: u64) -> Result<(), String> {
        if now_ms < self.clock_ms {
            return Err("The adapter clock moved backwards.".into());
        }
        self.clock_ms = now_ms;
        self.pointers
            .retain(|_, p| now_ms.saturating_sub(p.updated_ms) <= POINTER_TIMEOUT_MS);
        self.observations
            .retain(|_, e| now_ms.saturating_sub(e.captured_ms) <= MAX_AGE_MS);
        for request in self.requests.values_mut() {
            if !request.terminal()
                && (now_ms.saturating_sub(request.created_ms) > REQUEST_TIMEOUT_MS
                    || request.status == Status::Queued
                        && request
                            .observations
                            .iter()
                            .any(|id| !self.observations.contains_key(id)))
            {
                request.status = Status::Canceled;
            }
        }
        Ok(())
    }

    fn validate_evidence(
        &self,
        workspace: u64,
        evidence: &Evidence,
        now_ms: u64,
    ) -> Result<&Surface, String> {
        let surface = self.surface(&evidence.surface, workspace)?;
        let connection = self
            .connections
            .get(&evidence.device)
            .ok_or("Camera is disconnected.")?;
        if !valid_id(&evidence.id)
            || !(0.0..=1.0).contains(&evidence.confidence)
            || evidence.media.as_deref().is_some_and(|id| !valid_id(id))
            || surface.camera.as_deref() != Some(&evidence.device)
            || surface.calibration_revision != evidence.calibration_revision
            || connection.session != evidence.session
            || !connection.capture_enabled
            || evidence.sequence <= connection.sequence
            || evidence.captured_ms < connection.time_ms
            || evidence.captured_ms > now_ms
            || now_ms.saturating_sub(evidence.captured_ms) > MAX_AGE_MS
            || self.observations.contains_key(&evidence.id)
        {
            return Err("Invalid, stale, duplicate or unapproved camera evidence.".into());
        }
        Ok(surface)
    }

    /// Registers evidence from the current camera session and calibration.
    ///
    /// # Errors
    ///
    /// Returns the first clock, evidence, identity, or capacity diagnostic.
    pub fn observe(
        &mut self,
        workspace: u64,
        evidence: Evidence,
        now_ms: u64,
    ) -> Result<(), String> {
        self.advance(now_ms)?;
        self.validate_evidence(workspace, &evidence, now_ms)?;
        if self
            .requests
            .values()
            .any(|request| request.observations.contains(&evidence.id))
        {
            return Err("Observation identity is already in use.".into());
        }
        if self.observations.len() == MAX_OBSERVATIONS {
            return Err(
                "Observation capacity reached; wait for old observations to expire.".into(),
            );
        }
        let connection = self
            .connections
            .get_mut(&evidence.device)
            .ok_or("Camera is disconnected.")?;
        connection.sequence = evidence.sequence;
        connection.time_ms = evidence.captured_ms;
        self.observations.insert(evidence.id.clone(), evidence);
        Ok(())
    }

    /// Applies one deliberate pointer action and returns any commit.
    ///
    /// # Errors
    ///
    /// Returns the first clock, pointer, evidence, projection, or calibration diagnostic.
    pub fn interact(
        &mut self,
        workspace: u64,
        interaction: Interaction,
        now_ms: u64,
    ) -> Result<Option<Commit>, String> {
        self.advance(now_ms)?;
        if !valid_id(&interaction.pointer) {
            return Err("Invalid pointer identity.".into());
        }
        let evidence = &interaction.evidence;
        let existing = self.pointers.get(&interaction.pointer);
        let result = (|| {
            let surface = self.validate_evidence(workspace, evidence, now_ms)?;
            if self.blanked || !self.connections.contains_key(&surface.projector) {
                return Err("Projection is blanked or disconnected.".into());
            }
            if let Some(pointer) = existing {
                if pointer.surface != evidence.surface
                    || pointer.device != evidence.device
                    || pointer.session != evidence.session
                {
                    return Err("The pointer belongs to another device or surface.".into());
                }
            } else if interaction.phase != Phase::Begin {
                return Err("Begin a deliberate selection first.".into());
            }
            if interaction.phase == Phase::Begin
                && (existing.is_some() || self.pointers.len() >= MAX_POINTERS)
            {
                return Err("Pointer already selected or capacity reached.".into());
            }
            if evidence.confidence < MIN_SELECTION_CONFIDENCE || evidence.projected_content {
                return Err(
                    "Selection requires confident input independent of projected content.".into(),
                );
            }
            surface.camera_quad.map(interaction.camera_normalized, true)
        })();
        let local = match result {
            Ok(local) => local,
            Err(error) => {
                if existing
                    .is_some_and(|p| p.device == evidence.device && p.session == evidence.session)
                {
                    self.pointers.remove(&interaction.pointer);
                }
                return Err(error);
            }
        };
        let Interaction {
            evidence,
            pointer,
            phase,
            ..
        } = interaction;
        let connection = self
            .connections
            .get_mut(&evidence.device)
            .ok_or("Camera is disconnected.")?;
        connection.sequence = evidence.sequence;
        connection.time_ms = evidence.captured_ms;
        self.pointers.remove(&pointer);
        match phase {
            Phase::Begin | Phase::Update => {
                self.pointers.insert(
                    pointer,
                    Pointer {
                        surface: evidence.surface,
                        device: evidence.device,
                        session: evidence.session,
                        calibration_revision: evidence.calibration_revision,
                        local,
                        updated_ms: now_ms,
                    },
                );
                Ok(None)
            }
            Phase::End => Ok(Some(Commit {
                pointer,
                surface: evidence.surface,
                local,
            })),
            Phase::Cancel => Ok(None),
        }
    }

    /// Sets projection blanking and clears pointers when blanked.
    pub fn blank(&mut self, blanked: bool) {
        self.blanked = blanked;
        if blanked {
            self.pointers.clear();
        }
    }
}
