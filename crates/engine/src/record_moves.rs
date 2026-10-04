use crate::{Engine, EngineError};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use store::record_move::offers::{self, Bundle, Item, Origin, Preview, references as refs};
use store::sqlx::Row;

fn failure(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}
impl Engine {
    pub async fn preview_record_move(
        &self,
        record: &str,
        peer: &str,
    ) -> Result<(Preview, Bundle), EngineError> {
        let root = self.resolve(record).await?;
        let peer = self.resolve(peer).await?;
        self.move_contact(&peer, true).await?;
        let pool = &self.store.pool;
        let local = store::organs::local(pool)
            .await?
            .ok_or_else(|| failure("No local Organ"))?
            .uid;
        let rows = store::sqlx::query("SELECT uid,slug,head,kind,organ_uid,replica_root FROM record WHERE deleted_at IS NULL LIMIT 50001").fetch_all(pool).await?;
        if rows.len() > 50000 {
            return Err(failure(
                "The local Record index exceeds the move preview limit",
            ));
        }
        let mut aliases = BTreeMap::new();
        let mut items = BTreeMap::new();
        for row in rows {
            let uid: String = row.get("uid");
            aliases.insert(uid.clone(), uid.clone());
            if let Some(slug) = row.get::<Option<String>, _>("slug") {
                aliases.insert(slug, uid.clone());
            }
            items.insert(
                uid.clone(),
                (
                    Item {
                        uid,
                        title: row.get("head"),
                        kind: row.get("kind"),
                    },
                    row.get::<Option<String>, _>("organ_uid"),
                    row.get::<Option<String>, _>("replica_root"),
                ),
            );
        }
        let dependencies = offers::dependencies(pool).await?;
        let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (owner, raw) in &dependencies {
            let mut references = BTreeSet::new();
            refs(&serde_json::from_str(raw)?, &mut references);
            for reference in references {
                if let Some(uid) = aliases.get(&reference) {
                    dependents
                        .entry(uid.clone())
                        .or_default()
                        .insert(owner.clone());
                }
            }
        }
        let mut records = BTreeSet::from([root.clone()]);
        loop {
            if records.len() > offers::MAX_RECORDS {
                return Err(failure(
                    "This dependency set exceeds 128 Records; choose a smaller set",
                ));
            }
            let assertions =
                offers::table(pool, "record_assertion", "subject_uid", &records).await?;
            let incoming: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT subject_uid FROM record_assertion WHERE object_uid IN (SELECT value FROM json_each(?)) AND retracted_at IS NULL LIMIT 129")
                .bind(serde_json::to_string(&records)?).fetch_all(pool).await?;
            let rules = offers::table(pool, "recurrence", "record_uid", &records).await?;
            let rules_ids = offers::ids(&rules, "uid");
            let binding =
                offers::table(pool, "karma_field_binding", "rule_uid", &rules_ids).await?;
            let fields_ids = offers::ids(&binding, "field_uid");
            let fields = offers::table(pool, "karma_field", "uid", &fields_ids).await?;
            let readers: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT r.record_uid FROM karma_field_binding b JOIN recurrence r ON r.uid=b.rule_uid WHERE b.field_uid IN (SELECT value FROM json_each(?)) LIMIT 129")
                .bind(serde_json::to_string(&fields_ids)?).fetch_all(pool).await?;
            let mut candidates = offers::ids(&assertions, "object_uid");
            candidates.extend(incoming);
            candidates.extend(readers);
            for uid in &records {
                if let Some(owners) = dependents.get(uid) {
                    candidates.extend(owners.iter().cloned());
                }
            }
            for table in [&rules, &fields] {
                for row in &table.rows {
                    for cell in row {
                        if let store::snapshot::Value::Text(s) = cell {
                            refs(
                                &serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
                                &mut candidates,
                            );
                        }
                    }
                }
            }
            for (table, key) in [
                ("record_extension", "record_uid"),
                ("karma_program_revision", "program_uid"),
                ("karma_program", "record_uid"),
                ("karma_frequency_revision", "frequency_uid"),
                ("karma_frequency", "record_uid"),
                ("karma_rule_frequency", "recurrence_uid"),
            ] {
                let selection = if table == "karma_rule_frequency" {
                    &rules_ids
                } else {
                    &records
                };
                let table = offers::table(pool, table, key, selection).await?;
                for row in &table.rows {
                    for cell in row {
                        if let store::snapshot::Value::Text(s) = cell {
                            refs(
                                &serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
                                &mut candidates,
                            );
                        }
                    }
                }
            }
            let before = records.len();
            for candidate in candidates {
                if let Some(uid) = aliases.get(&candidate) {
                    records.insert(uid.clone());
                }
            }
            if records.len() == before {
                break;
            }
        }
        let hidden = store::visibility::hidden_from_organ(pool, &peer).await?;
        let mut selected = Vec::new();
        for uid in &records {
            let (item, origin, replica) = items
                .get(uid)
                .ok_or_else(|| failure("A move dependency is missing"))?;
            if !matches!(
                item.kind.as_str(),
                "plain" | "rule" | "program" | "frequency" | "protein"
            ) || origin.as_deref() != Some(local.as_str())
                || replica.is_some()
            {
                return Err(failure(format!(
                    "{} cannot move: only local Records, Karma definitions and Proteins are supported; account, executable, Transfer and replica data stay in their existing workflows",
                    item.title
                )));
            }
            if hidden.contains(uid) {
                return Err(failure("A move dependency is hidden from this contact"));
            }
            selected.push(item.clone());
        }
        let assertions = offers::table(pool, "record_assertion", "subject_uid", &records).await?;
        let rules = offers::table(pool, "recurrence", "record_uid", &records).await?;
        let rules_ids = offers::ids(&rules, "uid");
        let bindings = offers::table(pool, "karma_field_binding", "rule_uid", &rules_ids).await?;
        let field_ids = offers::ids(&bindings, "field_uid");
        let mut concepts = offers::ids(&assertions, "predicate_uid");
        concepts.extend(offers::ids(&assertions, "unit_uid"));
        let unit_uids: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT unit_uid FROM record WHERE uid IN (SELECT value FROM json_each(?)) AND unit_uid IS NOT NULL").bind(serde_json::to_string(&records)?).fetch_all(pool).await?;
        concepts.extend(unit_uids);
        let placed: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid IN (SELECT value FROM json_each(?)) AND place_uid IS NOT NULL)").bind(serde_json::to_string(&records)?).fetch_one(pool).await?;
        if placed {
            return Err(failure(
                "This move cannot carry Place dependencies. Remove the place from the selected Records before offering.",
            ));
        }
        let all_concepts: Vec<(String, String)> =
            store::sqlx::query_as("SELECT uid,canonical_name FROM concept LIMIT 10001")
                .fetch_all(pool)
                .await?;
        let mut references = BTreeSet::new();
        for row in &rules.rows {
            for cell in row {
                if let store::snapshot::Value::Text(s) = cell {
                    refs(
                        &serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
                        &mut references,
                    );
                }
            }
        }
        for (uid, name) in all_concepts {
            if references.contains(&uid) || references.contains(&name) {
                concepts.insert(uid);
            }
        }
        loop {
            let parents = offers::table(pool, "concept_parent", "concept_uid", &concepts).await?;
            let before = concepts.len();
            concepts.extend(offers::ids(&parents, "parent_uid"));
            if concepts.len() > 2048 {
                return Err(failure("Too many ontology dependencies"));
            }
            if concepts.len() == before {
                break;
            }
        }
        let mut tables = Vec::new();
        for (name, key) in offers::TABLES {
            let ids = match *name {
                "concept" | "concept_name" | "concept_parent" => &concepts,
                "recurrence_revision" | "karma_field_binding" | "karma_rule_frequency" => {
                    &rules_ids
                }
                "karma_field" => &field_ids,
                _ => &records,
            };
            let table = offers::table(pool, name, key, ids).await?;
            if *name == "record_extension"
                && table.rows.iter().any(|r| {
                    offers::text(&table, r, "namespace")
                        .is_some_and(|n| n.starts_with("lince.") && n != "lince.karma")
                })
            {
                return Err(failure(
                    "A dependency has private or executable configuration; this move cannot carry it",
                ));
            }
            tables.push(table);
        }
        let fact_uids: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM fact WHERE record_uid IN (SELECT value FROM json_each(?)) ORDER BY uid LIMIT 20001").bind(serde_json::to_string(&records)?).fetch_all(pool).await?;
        if fact_uids.len() > 20000 {
            return Err(failure("Too much origin evidence for one move"));
        }
        let mut facts = Vec::new();
        let mut origins = Vec::new();
        let cell = store::cells::local(pool)
            .await?
            .ok_or_else(|| failure("No local Cell"))?
            .uid;
        for uid in fact_uids {
            if let Some(fact) = store::facts::get(pool, &uid).await? {
                let stored: Option<(String, String)> = store::sqlx::query_as(
                    "SELECT organ_uid,cell_uid FROM fact_origin WHERE fact_uid=?",
                )
                .bind(&uid)
                .fetch_optional(pool)
                .await?;
                let (organ_uid,cell_uid) = match stored {
                    Some(origin) => origin,
                    None => store::sqlx::query_as("SELECT organ_uid,actor_cell FROM sync_op WHERE tbl='fact' AND uid=? ORDER BY seq LIMIT 1").bind(&uid).fetch_optional(pool).await?.unwrap_or_else(||(local.clone(),cell.clone())),
                };
                origins.push(Origin {
                    fact_uid: uid,
                    organ_uid,
                    cell_uid,
                });
                facts.push(fact);
            }
        }
        let dependency_hash = offers::dependency_hash(&dependencies, &tables[3])?;
        let bundle = Bundle {
            tables,
            facts,
            origins,
            dependency_hash,
        };
        let bytes = serde_json::to_vec(&bundle)?;
        if bytes.len() > offers::MAX_BYTES {
            return Err(failure("This move exceeds the 8 MiB direct delivery limit"));
        }
        let preview = Preview {
            root,
            hash: nucleus::fact::sha256_hex(&bytes),
            records: selected,
            assertions: assertions.rows.len(),
            karma_rules: rules.rows.len(),
            bytes: bytes.len(),
            dependencies: offers::manifest(&bundle),
        };
        if preview.dependencies.len() > 2048 || serde_json::to_vec(&preview)?.len() > 512 * 1024 {
            return Err(failure(
                "The complete preview exceeds 2,048 dependencies or 512 KiB; choose a smaller move set",
            ));
        }
        self.validate_move_bundle(&local, &preview, &bundle).await?;
        Ok((preview, bundle))
    }

    async fn move_contact(&self, peer: &str, outgoing: bool) -> Result<(), EngineError> {
        let contact = store::organs::contact(&self.store.pool, peer)
            .await?
            .ok_or_else(|| failure("The move party is no longer a contact"))?;
        if contact.trust != "known"
            || (outgoing && (!contact.sync_out || contact.scope_fields.is_some()))
            || (!outgoing && (!contact.sync_in || contact.accept_fields.is_some()))
        {
            return Err(EngineError::Forbidden("Moving a complete dependency set requires a known contact with full current send/receive permission".into()));
        }
        if outgoing && contact.delivery() == store::organs::Delivery::Mailbox {
            return Err(failure(
                "Record moves require direct delivery; choose Direct or Automatic for this contact",
            ));
        }
        let roster = self.roster_of(peer).await?.ok_or_else(|| {
            failure("Missing contact device list. Reconnect before resuming the move")
        })?;
        if !crate::roster::roster_signature_is_valid(&roster) {
            return Err(failure(
                "The contact device list expired. Reconnect before resuming the move",
            ));
        }
        Ok(())
    }

    pub async fn offer_record_move(
        &self,
        record: &str,
        peer: &str,
        expected: Option<&str>,
    ) -> Result<String, EngineError> {
        let peer = self.resolve(peer).await?;
        let (preview, bundle) = self.preview_record_move(record, &peer).await?;
        if expected != Some(preview.hash.as_str()) {
            return Err(failure(
                "Preview the complete move set first; its contents changed or no preview was provided",
            ));
        }
        let uid = nucleus::new_uid("move");
        offers::save(
            &self.store.pool,
            &uid,
            &peer,
            "outgoing",
            &preview,
            Some(&serde_json::to_string(&bundle)?),
        )
        .await?;
        self.notify_query_changed();
        Ok(uid)
    }

    pub async fn answer_record_move(&self, uid: &str, accept: bool) -> Result<(), EngineError> {
        let offer = offers::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| failure("No such move offer"))?;
        if offer.direction != "incoming" {
            return Err(failure("Only incoming offers can be accepted or declined"));
        }
        if accept {
            self.move_contact(&offer.peer, false).await?;
        }
        if !offers::transition(
            &self.store.pool,
            uid,
            "offered",
            if accept { "accepted" } else { "declined" },
        )
        .await?
        {
            return Err(failure("This offer is no longer waiting for an answer"));
        }
        if !accept {
            store::offers::refuse(
                &self.store.pool,
                store::offers::OfferKind::RecordMove,
                uid,
                &offer.peer,
            )
            .await?;
        }
        self.notify_query_changed();
        Ok(())
    }

    pub async fn cancel_record_move(&self, record: &str) -> Result<(), EngineError> {
        let pool = &self.store.pool;
        let uid: Option<String> = store::sqlx::query_scalar("SELECT uid FROM record_move_offer WHERE direction='outgoing' AND (uid=? OR root=?) AND state NOT IN ('complete','cancelled') ORDER BY created_at DESC LIMIT 1").bind(record).bind(record).fetch_optional(pool).await?;
        let uid = uid.ok_or_else(|| failure("No cancellable move offer"))?;
        let offer = offers::get(pool, &uid)
            .await?
            .ok_or_else(|| failure("No move offer"))?;
        if !matches!(offer.state.as_str(), "offered" | "changed")
            || !offers::transition(pool, &uid, &offer.state, "cancelled").await?
        {
            return Err(failure(
                "Cancellation closed when accepted delivery started; receipt recovery must finish",
            ));
        }
        self.notify_query_changed();
        Ok(())
    }

    pub async fn receive_move_offer(
        &self,
        peer: &str,
        uid: &str,
        preview: Preview,
    ) -> Result<String, EngineError> {
        self.move_contact(peer, false).await?;
        if uid.len() > 128
            || preview.records.is_empty()
            || preview.records.len() > offers::MAX_RECORDS
            || preview.dependencies.len() > 2048
            || serde_json::to_vec(&preview)?.len() > 512 * 1024
            || preview.bytes > offers::MAX_BYTES
            || preview.hash.len() != 64
            || !preview.records.iter().any(|r| r.uid == preview.root)
            || preview
                .records
                .iter()
                .any(|r| r.uid.len() > 128 || r.title.len() > 4096)
        {
            return Err(failure("Invalid move preview"));
        }
        if let Some(held) = offers::get(&self.store.pool, uid).await? {
            if held.peer != peer || held.direction != "incoming" || held.preview != preview {
                return Err(failure("Move offer identity reused"));
            }
            return Ok(if held.state == "declined" {
                "offered".into()
            } else {
                held.state
            });
        }
        let cancelled: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM offer_local_outcome WHERE kind='record-move' AND subject_uid=? AND other_party=? AND outcome='cancelled')").bind(uid).bind(peer).fetch_one(&self.store.pool).await?;
        if cancelled {
            return Ok("cancelled".into());
        }
        if store::offers::refused_by_party(
            &self.store.pool,
            store::offers::OfferKind::RecordMove,
            peer,
        )
        .await?
        {
            return Ok("offered".into());
        }
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_move_offer WHERE direction='incoming' AND state IN ('offered','accepted')").fetch_one(&self.store.pool).await?;
        if count >= 128 {
            return Err(failure("Too many pending moves"));
        }
        offers::save(&self.store.pool, uid, peer, "incoming", &preview, None).await?;
        self.notify_query_changed();
        Ok("offered".into())
    }

    pub async fn receive_move_cancel(&self, peer: &str, uid: &str) -> Result<(), EngineError> {
        if uid.is_empty() || uid.len() > 128 {
            return Err(failure("Invalid move cancellation"));
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        let held =
            store::sqlx::query("SELECT peer,direction,state FROM record_move_offer WHERE uid=?")
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(held) = held {
            if held.get::<String, _>("peer") != peer
                || held.get::<String, _>("direction") != "incoming"
            {
                return Err(EngineError::Forbidden("Move party mismatch".into()));
            }
            if matches!(
                held.get::<String, _>("state").as_str(),
                "offered" | "accepted" | "declined"
            ) {
                store::sqlx::query(
                    "UPDATE record_move_offer SET state='cancelled',updated_at=? WHERE uid=?",
                )
                .bind(nucleus::execution::now().to_rfc3339())
                .bind(uid)
                .execute(&mut *tx)
                .await?;
            }
        } else {
            store::sqlx::query("INSERT INTO offer_local_outcome(kind,subject_uid,other_party,outcome,at) VALUES ('record-move',?,?,'cancelled',?) ON CONFLICT(kind,subject_uid,other_party) DO UPDATE SET outcome='cancelled',at=excluded.at").bind(uid).bind(peer).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(())
    }

    pub async fn receive_move_bundle(
        &self,
        peer: &str,
        uid: &str,
        bundle: Bundle,
    ) -> Result<String, EngineError> {
        self.move_contact(peer, false).await?;
        let pool = &self.store.pool;
        let offer = offers::get(pool, uid)
            .await?
            .ok_or_else(|| failure("No accepted offer"))?;
        let raw = serde_json::to_vec(&bundle)?;
        if offer.peer != peer
            || offer.direction != "incoming"
            || !matches!(offer.state.as_str(), "accepted" | "received")
            || raw.len() != offer.preview.bytes
            || nucleus::fact::sha256_hex(&raw) != offer.preview.hash
        {
            return Err(failure(
                "The received bundle does not match an accepted preview",
            ));
        }
        if offer.state == "received" {
            return Ok(offer.preview.hash);
        }
        self.validate_move_bundle(peer, &offer.preview, &bundle)
            .await?;
        let mut tx = store::write_tx(pool).await?;
        self.move_permission_on(&mut tx, peer, false).await?;
        for item in &offer.preview.records {
            let exists: bool =
                store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=?)")
                    .bind(&item.uid)
                    .fetch_one(&mut *tx)
                    .await?;
            if exists {
                return Err(failure(
                    "A destination Record identity already exists; source retained",
                ));
            }
        }
        let accepted = store::sqlx::query("UPDATE record_move_offer SET state='received',error=NULL,updated_at=? WHERE uid=? AND state='accepted'").bind(nucleus::execution::now().to_rfc3339()).bind(uid).execute(&mut *tx).await?.rows_affected();
        if accepted != 1 {
            return Err(failure("Acceptance changed before durable import"));
        }
        store::sqlx::query("PRAGMA defer_foreign_keys=ON")
            .execute(&mut *tx)
            .await?;
        for table in &bundle.tables {
            if table.name != "record_revision" {
                offers::insert_table(&mut tx, table).await?;
            }
        }
        for (fact, origin) in bundle.facts.iter().zip(&bundle.origins) {
            let exists = store::facts::exists(&mut tx, &fact.uid).await?;
            if exists {
                return Err(failure(
                    "A destination Fact identity already exists; source retained",
                ));
            }
            let prev = store::facts::last_hash(&mut tx).await?;
            let news = nucleus::NewFact {
                uid: Some(fact.uid.clone()),
                record_uid: fact.record_uid.clone(),
                delta: fact.delta,
                at: Some(fact.at),
                actor_uid: fact.actor_uid.clone(),
                cause: nucleus::Cause {
                    kind: nucleus::CauseKind::Sync,
                    uid: Some(peer.into()),
                },
                payload: fact.payload.clone(),
            };
            let local = nucleus::fact::seal(news, &prev, nucleus::execution::now());
            store::facts::insert(&mut tx, &local).await?;
            store::facts::retain_origin(&mut tx, fact, &origin.organ_uid, &origin.cell_uid).await?;
        }
        for item in &offer.preview.records {
            store::sqlx::query("UPDATE recurrence SET state='paused' WHERE record_uid=?")
                .bind(&item.uid)
                .execute(&mut *tx)
                .await?;
            store::sqlx::query("UPDATE karma_program SET status='paused',active_revision_hash=NULL WHERE record_uid=? AND status='active'").bind(&item.uid).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE karma_frequency SET status='paused',active_revision_hash=NULL,active_activation_hash=NULL WHERE record_uid=? AND status='active'").bind(&item.uid).execute(&mut *tx).await?;
        }
        let revisions = bundle
            .tables
            .iter()
            .find(|t| t.name == "record_revision")
            .expect("validated table set");
        let index = revisions
            .columns
            .iter()
            .position(|c| c == "revision")
            .ok_or_else(|| failure("Missing Record revision"))?;
        for row in &revisions.rows {
            let store::snapshot::Value::Integer(revision) = row[index] else {
                return Err(failure("Invalid Record revision"));
            };
            store::sqlx::query("UPDATE record_revision SET revision=? WHERE record_uid=?")
                .bind(revision)
                .bind(offers::text(revisions, row, "record_uid"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        self.notify_karma_deadline_change();
        Ok(offer.preview.hash)
    }

    async fn validate_move_bundle(
        &self,
        peer: &str,
        preview: &Preview,
        bundle: &Bundle,
    ) -> Result<(), EngineError> {
        if bundle.tables.len() != offers::TABLES.len()
            || bundle
                .tables
                .iter()
                .zip(offers::TABLES)
                .any(|(t, (n, _))| t.name != *n)
        {
            return Err(failure("Invalid move table set"));
        }
        let records: BTreeSet<_> = preview.records.iter().map(|r| r.uid.clone()).collect();
        for (table, (name, _)) in bundle.tables.iter().zip(offers::TABLES) {
            let columns: Vec<String> =
                store::sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                    .bind(*name)
                    .fetch_all(&self.store.pool)
                    .await?;
            if columns != table.columns || table.rows.iter().any(|r| r.len() != columns.len()) {
                return Err(failure("Move schema does not match"));
            }
        }
        let record = &bundle.tables[3];
        if offers::ids(record, "uid") != records
            || record.rows.iter().any(|r| {
                offers::text(record, r, "organ_uid") != Some(peer)
                    || offers::text(record, r, "replica_root").is_some()
                    || record
                        .columns
                        .iter()
                        .position(|c| c == "deleted_at")
                        .is_none_or(|i| r[i] != store::snapshot::Value::Null)
                    || !matches!(
                        offers::text(record, r, "kind"),
                        Some("plain" | "rule" | "program" | "frequency" | "protein")
                    )
            })
        {
            return Err(failure("The move contains foreign or unsupported Records"));
        }
        for item in &preview.records {
            let row = record
                .rows
                .iter()
                .find(|r| offers::text(record, r, "uid") == Some(item.uid.as_str()))
                .ok_or_else(|| failure("Preview Record is missing"))?;
            if offers::text(record, row, "head") != Some(item.title.as_str())
                || offers::text(record, row, "kind") != Some(item.kind.as_str())
            {
                return Err(failure(
                    "Record identity does not match the accepted preview",
                ));
            }
        }
        if offers::manifest(bundle) != preview.dependencies {
            return Err(failure(
                "Karma and Assertion dependencies do not match the accepted preview",
            ));
        }
        let table = |name: &str| {
            bundle
                .tables
                .iter()
                .find(|t| t.name == name)
                .expect("validated table set")
        };
        let assertion = table("record_assertion");
        if assertion.rows.iter().any(|r| {
            offers::text(assertion, r, "subject_uid").is_none_or(|s| !records.contains(s))
                || offers::text(assertion, r, "object_uid").is_some_and(|s| !records.contains(s))
        }) {
            return Err(failure("An Assertion escapes the move set"));
        }
        let rules = offers::ids(table("recurrence"), "uid");
        if offers::ids(table("record_revision"), "record_uid") != records {
            return Err(failure("Missing Record revision identities"));
        }
        let fields = offers::ids(table("karma_field_binding"), "field_uid");
        let concepts = offers::ids(table("concept"), "uid");
        for (t, (_, key)) in bundle.tables.iter().zip(offers::TABLES) {
            let allowed = match t.name.as_str() {
                "concept" | "concept_name" | "concept_parent" => &concepts,
                "recurrence_revision" | "karma_field_binding" | "karma_rule_frequency" => &rules,
                "karma_field" => &fields,
                _ => &records,
            };
            if offers::ids(t, key).iter().any(|id| !allowed.contains(id)) {
                return Err(failure("Dependency row escapes the move set"));
            }
        }
        for name in ["karma_program_revision", "karma_frequency_revision"] {
            let revisions = table(name);
            for row in &revisions.rows {
                let raw = offers::text(revisions, row, "ast_json")
                    .ok_or_else(|| failure("Missing Karma definition"))?;
                let claimed = offers::text(revisions, row, "revision_hash")
                    .ok_or_else(|| failure("Missing Karma revision hash"))?;
                if name == "karma_program_revision" {
                    let ast: nucleus::karma::ProgramAst = serde_json::from_str(raw)?;
                    let proof = nucleus::karma::prove_program(&ast);
                    if proof.revision_hash.as_ref().map(|h| h.as_str()) != Some(claimed)
                        || proof.status != nucleus::karma::ProofStatus::Accepted
                    {
                        return Err(failure(
                            "Moved Karma Program does not prove or match its revision",
                        ));
                    }
                } else {
                    let ast: nucleus::karma::FrequencyAst = serde_json::from_str(raw)?;
                    let compiled = ast
                        .compile(&BTreeMap::new())
                        .map_err(|e| failure(e.to_string()))?;
                    if compiled.revision_hash.as_str() != claimed {
                        return Err(failure("Moved Karma Frequency does not match its revision"));
                    }
                }
            }
        }
        let ext = table("record_extension");
        if ext.rows.iter().any(|r| {
            offers::text(ext, r, "namespace")
                .is_some_and(|n| n.starts_with("lince.") && n != "lince.karma")
        }) {
            return Err(failure("Private or executable configuration cannot move"));
        }
        let doc = table("record_doc");
        for row in &doc.rows {
            let snapshot = doc
                .columns
                .iter()
                .position(|c| c == "snapshot")
                .and_then(|i| row.get(i));
            match snapshot {
                Some(store::snapshot::Value::Blob(bytes)) => {
                    crate::collab_guard::AcceptedDoc::from_trusted_snapshot(
                        bytes,
                        &crate::collab_guard::Limits::default(),
                    )
                    .map_err(|e| failure(e.to_string()))?;
                }
                _ => return Err(failure("Invalid move document")),
            }
        }
        for fact in &bundle.facts {
            if !records.contains(&fact.record_uid)
                || !nucleus::fact::verify_chain_step(fact)
                || (fact.signature.is_some()
                    && !crate::trust::verify_fact(&self.store, fact).await?)
            {
                return Err(failure("Move origin evidence does not verify"));
            }
        }
        if bundle.origins.len() != bundle.facts.len()
            || bundle
                .origins
                .iter()
                .zip(&bundle.facts)
                .any(|(origin, fact)| {
                    origin.fact_uid != fact.uid
                        || origin.organ_uid.is_empty()
                        || origin.organ_uid.len() > 128
                        || origin.cell_uid.is_empty()
                        || origin.cell_uid.len() > 128
                })
        {
            return Err(failure("Invalid move origin identities"));
        }
        Ok(())
    }

    pub async fn claim_record_move(&self, uid: &str) -> Result<bool, EngineError> {
        let offer = offers::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| failure("Unknown move"))?;
        if offer.direction != "outgoing" {
            return Err(failure("Only the source can claim delivery"));
        }
        let (current, _) = self.preview_record_move(&offer.root, &offer.peer).await?;
        if current.hash != offer.preview.hash {
            offers::transition(&self.store.pool, uid, "offered", "changed").await?;
            offers::error(
                &self.store.pool,
                uid,
                "Source changed after preview; cancel and preview again",
            )
            .await?;
            return Ok(false);
        }
        Ok(offers::transition(&self.store.pool, uid, "offered", "transferring").await?)
    }

    pub async fn finish_record_move(&self, uid: &str, hash: &str) -> Result<(), EngineError> {
        let pool = &self.store.pool;
        let offer = offers::get(pool, uid)
            .await?
            .ok_or_else(|| failure("Unknown move"))?;
        if offer.direction != "outgoing"
            || hash != offer.preview.hash
            || !matches!(offer.state.as_str(), "transferring" | "complete")
        {
            return Err(failure(
                "A matching durable receipt after acceptance is required",
            ));
        }
        if offer.state == "complete" {
            return Ok(());
        }
        let (current, _) = self.preview_record_move(&offer.root, &offer.peer).await?;
        if current.hash != hash {
            return Err(failure(
                "Source changed during delivery; both copies retained for review",
            ));
        }
        let bundle: Bundle = serde_json::from_str(&offers::payload(pool, uid).await?)?;
        let mut tx = store::write_tx(pool).await?;
        self.move_permission_on(&mut tx, &offer.peer, true).await?;
        offers::unchanged(&mut tx, &bundle).await?;
        let committed=store::sqlx::query("UPDATE record_move_offer SET state='complete',payload=NULL,error=NULL,updated_at=? WHERE uid=? AND state='transferring'").bind(nucleus::execution::now().to_rfc3339()).bind(uid).execute(&mut *tx).await?.rows_affected();
        if committed != 1 {
            return Err(failure("Move state changed before completion"));
        }
        for item in &offer.preview.records {
            store::records::mark_deleted_on(&mut tx, &item.uid).await?;
            store::sqlx::query("UPDATE recurrence SET state='paused' WHERE record_uid=?")
                .bind(&item.uid)
                .execute(&mut *tx)
                .await?;
            store::sqlx::query("UPDATE record_assertion SET retracted_at=? WHERE subject_uid=? AND retracted_at IS NULL").bind(nucleus::execution::now().to_rfc3339()).bind(&item.uid).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE karma_program SET status='paused',active_revision_hash=NULL WHERE record_uid=? AND status='active'").bind(&item.uid).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE karma_frequency SET status='paused',active_revision_hash=NULL,active_activation_hash=NULL WHERE record_uid=? AND status='active'").bind(&item.uid).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        self.notify_karma_deadline_change();
        Ok(())
    }

    async fn move_permission_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        peer: &str,
        outgoing: bool,
    ) -> Result<(), EngineError> {
        let contact=store::sqlx::query("SELECT trust,sync_out,sync_in,scope_fields,accept_fields,mode FROM organ_contact WHERE record_uid=?").bind(peer).fetch_optional(&mut **tx).await?.ok_or_else(||failure("Move contact was forgotten"))?;
        let direction = if outgoing { "sync_out" } else { "sync_in" };
        let scope = if outgoing {
            "scope_fields"
        } else {
            "accept_fields"
        };
        if contact.get::<String, _>("trust") != "known"
            || contact.get::<i64, _>(direction) == 0
            || contact.get::<Option<String>, _>(scope).is_some()
            || (outgoing && contact.get::<String, _>("mode") == "mailbox")
        {
            return Err(EngineError::Forbidden(
                "Move permissions changed before commitment; source retained".into(),
            ));
        }
        Ok(())
    }

    pub async fn pending_offer_status(&self) -> Result<Value, EngineError> {
        let offers = store::offers::pending(&self.store.pool).await?;
        let pending: Vec<_>=offers.into_iter().map(|o|json!({"kind":o.kind.as_str(),"direction":o.direction.as_str(),"subject_uid":o.subject_uid,"title":o.title,"other_party":o.other_party,"created_at":o.created_at,"previously_refused":o.previously_refused})).collect();
        let moves = offers::list(&self.store.pool).await?;
        let refusals: Vec<_>=store::offers::refusals(&self.store.pool).await?.into_iter().map(|r|json!({"kind":r.kind,"subject_uid":r.subject_uid,"other_party":r.other_party,"at":r.at,"until":r.until})).collect();
        let transfers: Vec<Value> = store::sqlx::query("SELECT i.*,t.revision FROM transfer_invitation i JOIN transfer t ON t.record_uid=i.transfer_uid ORDER BY i.created_at DESC LIMIT 100").fetch_all(&self.store.pool).await?.into_iter().map(|r|json!({"uid":r.get::<String,_>("uid"),"transfer":r.get::<String,_>("transfer_uid"),"person":r.get::<String,_>("addressed_person_uid"),"status":r.get::<String,_>("status"),"revision":r.get::<i64,_>("revision")})).collect();
        let replicas: Vec<Value> = store::sqlx::query("SELECT root_record,contact_organ,state FROM replica_grant ORDER BY created_at DESC LIMIT 100").fetch_all(&self.store.pool).await?.into_iter().map(|r|json!({"root":r.get::<String,_>("root_record"),"peer":r.get::<String,_>("contact_organ"),"state":r.get::<String,_>("state")})).collect();
        Ok(
            json!({"pending":pending,"moves":moves,"refusals":refusals,"transfers":transfers,"replicas":replicas,"outcomes":store::offers::local_outcomes(&self.store.pool).await?}),
        )
    }
}
