use super::*;

fn exact_float(value: nucleus::DecimalValue) -> Result<f64, EngineError> {
    let number = value.to_f64();
    if !number.is_finite()
        || nucleus::DecimalValue::from_f64_lossy(number)
            .map_err(invalid)?
            .exact_numeric_cmp(value)
            != std::cmp::Ordering::Equal
    {
        return Err(invalid(
            "This quantity cannot be represented exactly by the Transfer terms",
        ));
    }
    Ok(number)
}

impl Engine {
    pub(super) async fn refresh_stock_offer(
        &self,
        saved: &mut Saved,
        preview: &Preview,
        basis: &str,
        source: &nucleus::transfer::karma::Snapshot,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let Recipe::Offer {
            transfer_uid,
            promise_uid,
            mode,
            ..
        } = saved.review.definition.recipe.clone()
        else {
            return Err(invalid("Choose an offer policy"));
        };
        let revision = saved.revision;
        if source.revision != saved.managed_revision {
            saved.enabled = false;
            saved.reason = "Transfer terms changed outside this policy; review again".into();
            storage::save(self, saved, revision).await?;
            return Ok(());
        }
        if preview.frozen
            || mode == Mode::PreserveOffer
            || number(&preview.offered)?.exact_numeric_cmp(number(&preview.proposed)?)
                == std::cmp::Ordering::Equal
        {
            if saved.reason != preview.reason {
                saved.reason = preview.reason.clone();
                storage::save(self, saved, revision).await?;
            }
            return Ok(());
        }
        self.require_transfer_draft_creator(&transfer_uid, actor)
            .await?;
        self.require_transfer_origin_authority(&transfer_uid)
            .await?;
        let signer = self
            .transfer_person_signer(&saved.review.definition.controller_uid, None)
            .await?;
        let hash=nucleus::karma::canonical_hash("lince.surplus-revision.v1",&json!({"policy":saved.review.definition.uid,"review":saved.commitment,"revision":source.revision,"basis":basis,"proposed":preview.proposed})).map_err(invalid)?;
        let request = format!("stock-offer:{}", hash.as_str());
        let mut input = store::transfers::publication_input(
            &self.store.pool,
            &transfer_uid,
            source.revision,
            &saved.review.definition.controller_uid,
            &request,
        )
        .await?;
        let original = store::transfers::get(&self.store.pool, &transfer_uid)
            .await?
            .ok_or_else(|| invalid("Offer unavailable"))?;
        input.terms.visibility = original.visibility;
        input.terms.max_proximity = original.max_proximity;
        let amount = number(&preview.proposed)?;
        if amount.is_zero() {
            input.promises.clear();
        } else {
            let promise = input
                .promises
                .iter_mut()
                .find(|promise| promise.uid.as_deref() == Some(promise_uid.as_str()))
                .ok_or_else(|| invalid("Outgoing promise changed"))?;
            promise.delta = -exact_float(amount)?;
        }
        let guard = nucleus::transfer::karma::Guard::exact(source);
        let commit = store::transfers::revise_whole_draft_guarded(
            &self.store.pool,
            input,
            &guard,
            (&saved.review.definition.stock_uid, basis),
            now,
            Some(saved.review.definition.controller_uid.clone()),
            |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
        )
        .await?;
        let mut outcome = ActionOutcome::default();
        self.apply_transfer_revision_commit(commit, source.revision, &transfer_uid, &mut outcome)
            .await?;
        saved.managed_revision = source.revision + 1;
        saved.reason = if amount.is_zero() {
            saved.enabled = false;
            "No surplus remains; the unaccepted bundle was withdrawn".into()
        } else {
            format!(
                "Unaccepted offer revised to {} through ordinary Transfer terms",
                preview.proposed
            )
        };
        storage::save(self, saved, revision).await?;
        Ok(())
    }

    pub(super) async fn refresh_stock_need(
        &self,
        saved: &mut Saved,
        preview: &Preview,
        basis: &str,
        actor: Option<&str>,
        _now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        if let Some(pending) = &saved.pending_need {
            self.transfer_karma_snapshot(pending, actor).await?;
            if !store::location::transfer_ended(&self.store.pool, pending).await? {
                return Ok(());
            }
        }
        let amount = number(&preview.proposed)?;
        if amount.is_zero() {
            return Ok(());
        }
        let Recipe::Replenish {
            upstream_transfer_uid,
            ..
        } = &saved.review.definition.recipe
        else {
            return Err(invalid("Choose a replenishment policy"));
        };
        self.require_permission(actor, "record:create").await?;
        self.require_permission(actor, "transfer:create").await?;
        self.transfer_person_signer(&saved.review.definition.controller_uid, None)
            .await?;
        let event = crate::rule_runtime::EFFECT_OCCURRENCE
            .try_with(|event| event.event_id.clone())
            .unwrap_or_else(|_| saved.commitment.clone());
        let hash = nucleus::karma::canonical_hash(
            "lince.stock-replenishment.v1",
            &json!({"policy":saved.review.definition.uid,"review":saved.commitment,"event":event}),
        )
        .map_err(invalid)?;
        let hash = hash.as_str();
        let millis = u64::from_str_radix(&hash[..12], 16).map_err(invalid)?;
        let entropy = u128::from_str_radix(&hash[12..32], 16).map_err(invalid)?;
        let need = format!("r_{}", nucleus::ulid_from(millis, entropy));
        let request = format!("stock-replenish:{hash}");
        let quantity = store::exact::negate(amount)?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        for permission in [
            "record:read",
            "record:update",
            "transfer:read",
            "transfer:create",
        ] {
            self.require_permission_on(&mut tx, actor, permission)
                .await?;
        }
        let balance =
            store::transfer_balances::read_on(&mut tx, &saved.review.definition.stock_uid).await?;
        let unit: Option<String> = store::sqlx::query_scalar(
            "SELECT unit_uid FROM record WHERE uid=? AND deleted_at IS NULL",
        )
        .bind(&saved.review.definition.stock_uid)
        .fetch_one(&mut *tx)
        .await?;
        let current_basis = nucleus::karma::canonical_hash(
            "lince.stock-policy-basis.v1",
            &json!({"balance":balance,"unit":unit}),
        )
        .map_err(invalid)?;
        if current_basis.as_str() != basis {
            return Err(invalid(
                "Stock or reservations changed before replenishment; recompute the proposal",
            ));
        }
        let existing: Option<(String, String)> = store::sqlx::query_as(
            "SELECT kind,head FROM record WHERE uid=? AND deleted_at IS NULL",
        )
        .bind(&need)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((kind, head)) = existing {
            if kind != "plain" || head != "Stock replenishment need" {
                return Err(invalid(
                    "Replenishment request identity conflicts with another Record",
                ));
            }
        } else {
            let organ:String=store::sqlx::query_scalar("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL").fetch_one(&mut *tx).await?;
            store::records::create_with_uid_on(
                &mut tx,
                &need,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Stock replenishment need",
                    body: "Review meanings and private effects with the linked Transfer",
                    quantity,
                },
                &organ,
                None,
            )
            .await?;
            store::records::set_unit_on(&mut tx, &need, saved.review.unit_uid.as_deref()).await?;
        }
        tx.commit().await?;
        let source = self
            .transfer_karma_snapshot(upstream_transfer_uid, actor)
            .await?;
        let candidates = source
            .participants
            .keys()
            .filter(|person| *person != &saved.review.definition.controller_uid)
            .cloned()
            .collect::<Vec<_>>();
        let supplier =
            if candidates.len() == 1 && self.may_read_record(actor, &candidates[0]).await? {
                Some(candidates[0].clone())
            } else {
                None
            };
        let exchange=supplier.as_ref().map(|supplier|json!({"uid":format!("stock:{hash}"),"giver":supplier,"receiver":saved.review.definition.controller_uid}));
        let action: Action = serde_json::from_value(
            json!({"action":"create-transfer-draft","request_id":request,"creator":saved.review.definition.controller_uid,"head":"Replenish stock","source":need,"visibility":"hidden","reserve_default":"none","require_confirmation":true,"invitees":supplier.iter().collect::<Vec<_>>(),"promises":[{"uid":format!("replenish:{hash}"),"record":"","party":supplier,"open":supplier.is_none(),"delta":-exact_float(amount)?,"unit":saved.review.unit_uid,"item":{"title":"Requested stock contribution","exchange":exchange}}],"dependencies":[{"scope":"transfer","upstream_kind":"transfer","upstream":upstream_transfer_uid,"required_state":"agreed"}]}),
        )?;
        let transfer = Box::pin(self.act(action, actor.map(str::to_owned)))
            .await?
            .created
            .ok_or_else(|| invalid("Replenishment Transfer unavailable"))?;
        let revision = saved.revision;
        saved.pending_need = Some(transfer);
        saved.reason = format!(
            "Proposed {} once; stock changes through reviewed ordinary Transfer effects",
            preview.proposed
        );
        storage::save(self, saved, revision).await?;
        Ok(())
    }
}
