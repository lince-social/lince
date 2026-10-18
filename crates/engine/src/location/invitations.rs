use super::*;

pub(super) struct Lease {
    pub(super) invitation: Invitation,
    pub(super) deadline: Instant,
}

impl Engine {
    pub(super) async fn location_prepare_invitation(
        &self,
        person: &str,
        record: &str,
        recipient: &str,
        duration_seconds: u32,
        radius_metres: Option<u32>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        if duration_seconds == 0
            || duration_seconds > MAX_DURATION_SECONDS
            || radius_metres.is_some_and(|radius| !nucleus::place::valid_area_radius(radius))
        {
            return Err(invalid(
                "Choose a bounded invitation duration and area radius",
            ));
        }
        self.location_manage(person, record, actor).await?;
        self.location_identity(recipient, None).await?;
        self.refuse_unreadable(actor, &[recipient.into()]).await?;
        let node = self.location_node_id()?;
        let mut state = self.location.state.lock().await;
        state
            .invitations
            .retain(|_, invitation| invitation.deadline > Instant::now());
        let session = state
            .sessions
            .get(record)
            .filter(|session| session.lease.settings.controller_uid == person)
            .ok_or_else(|| invalid("Start this Record's sharing before making an invitation"))?;
        let now = Instant::now();
        let deadline = session
            .deadline
            .min(now + Duration::from_secs(u64::from(duration_seconds)));
        let remaining = deadline - now;
        if remaining.is_zero() {
            return Err(invalid("This sharing session has ended"));
        }
        if state.invitations.len() >= 256
            || state
                .invitations
                .values()
                .filter(|lease| lease.invitation.record_uid == record)
                .count()
                >= MAX_RECIPIENTS
        {
            return Err(invalid("The active location invitation limit was reached"));
        }
        let grant = InvitationGrant {
            uid: nucleus::new_uid("li"),
            issuer: person.into(),
            recipient: recipient.into(),
            expires_at_ms: now_ms().saturating_add(remaining.as_millis() as i64),
            radius_metres,
        };
        let invitation = Invitation {
            record_uid: record.into(),
            authority_node_id: node,
            grant: Some(grant.clone()),
        };
        let reference = invitation.reference().map_err(invalid)?;
        state.invitations.insert(
            grant.uid,
            Lease {
                invitation: invitation.clone(),
                deadline,
            },
        );
        Ok(
            json!({"invitation":invitation,"reference":reference,"remaining_millis":remaining.as_millis() as u64,"notice":"This reference does not grant Record access or override location exclusions. Review the ordinary private conversation before sending it."}),
        )
    }

    pub(super) async fn location_read_invitation(
        &self,
        person: &str,
        organ: &str,
        invitation: &Invitation,
    ) -> Result<Value, EngineError> {
        invitation.validate().map_err(invalid)?;
        let grant = invitation
            .grant
            .as_ref()
            .ok_or_else(|| denied("Use normal viewing for a manual reference"))?;
        if grant.recipient != person || invitation.authority_node_id != self.location_node_id()? {
            return Err(denied(
                "This invitation names another recipient or authority",
            ));
        }
        {
            let state = self.location.state.lock().await;
            if !state.invitations.get(&grant.uid).is_some_and(|lease| {
                lease.invitation == *invitation && lease.deadline > Instant::now()
            }) {
                return Err(denied("This invitation expired or is unavailable"));
            }
        }
        let mut view = self
            .location_person_view_scoped(person, &invitation.record_uid, organ, Some(grant))
            .await?;
        let state = self.location.state.lock().await;
        let retained = state
            .invitations
            .get(&grant.uid)
            .filter(|lease| lease.invitation == *invitation && lease.deadline > Instant::now())
            .ok_or_else(|| denied("The invitation expired while reading"))?;
        let remaining = (retained.deadline - Instant::now()).as_millis() as u64;
        view.remaining_millis = Some(
            view.remaining_millis
                .map_or(remaining, |session| session.min(remaining)),
        );
        view.expires_at_ms=view.expires_at_ms.map(|_|grant.expires_at_ms);
        Ok(serde_json::to_value(view)?)
    }
}
