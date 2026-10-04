use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::sand_package::{self as model, Command, Identity, Manifest, Package, Query, Response};

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

impl Engine {
    pub async fn sand_package_command(
        &self,
        command: Command,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, command.permission()).await?;
        if actor.is_some() {
            return Err(EngineError::Forbidden(
                "Package libraries currently require the local interface session.".into(),
            ));
        }
        let response = match command {
            Command::List { organ, offset } => match organ {
                Some(organ) => self.remote_packages(&organ, Query::List { offset }).await?,
                None => self.package_catalogue(false, offset).await?,
            },
            Command::Inspect { organ, identity } => {
                identity.validate().map_err(invalid)?;
                match organ {
                    Some(organ) => {
                        self.remote_packages(&organ, Query::Inspect { identity })
                            .await?
                    }
                    None => {
                        let stored = store::sand_packages::get(&self.store.pool, &identity, false)
                            .await?
                            .ok_or_else(|| invalid("Package is unavailable."))?;
                        let origin_verified = self.package_origin_verified(&stored.package).await?;
                        Response::Package {
                            package: stored.package,
                            public: stored.public,
                            origin_verified,
                        }
                    }
                }
            }
            Command::Save {
                record,
                kind,
                licenses,
                credits,
            } => {
                let package = self
                    .snapshot_package(&record, kind, licenses, credits)
                    .await?;
                if !store::sand_packages::save(&self.store.pool, &package, true, None).await? {
                    return Err(invalid(
                        "Package version already contains different content; save again.",
                    ));
                }
                Response::Saved {
                    identity: package.manifest.identity,
                }
            }
            Command::Enable { identity } => {
                identity.validate().map_err(invalid)?;
                let stored = store::sand_packages::get(&self.store.pool, &identity, false)
                    .await?
                    .ok_or_else(|| invalid("Receive the package into the local library first."))?;
                let verified = self.package_origin_verified(&stored.package).await?;
                if !verified {
                    return Err(invalid(
                        "The original Organ signing key is not verified on this device.",
                    ));
                }
                stored.package.validate_execution().map_err(invalid)?;
                Response::Package {
                    package: stored.package,
                    public: stored.public,
                    origin_verified: true,
                }
            }
            Command::SetPublic { identity, public } => {
                identity.validate().map_err(invalid)?;
                let stored = store::sand_packages::get(&self.store.pool, &identity, false)
                    .await?
                    .ok_or_else(|| invalid("Package is unavailable."))?;
                if public && !self.package_origin_verified(&stored.package).await? {
                    return Err(invalid(
                        "Verify the package's original Organ identity before publishing it.",
                    ));
                }
                store::sand_packages::set_public(&self.store.pool, &identity, public).await?;
                Response::Saved { identity }
            }
            Command::Receive { organ, identity } => {
                identity.validate().map_err(invalid)?;
                let Response::Package { package, .. } = self
                    .remote_packages(
                        &organ,
                        Query::Inspect {
                            identity: identity.clone(),
                        },
                    )
                    .await?
                else {
                    return Err(invalid("Invalid package response."));
                };
                package.validate().map_err(invalid)?;
                if package.manifest.identity != identity || !signature_valid(&package)? {
                    return Err(invalid("Package identity or signature does not match."));
                }
                let verified = self.package_origin_verified(&package).await?;
                self.require_package_contact(&organ).await?;
                if !store::sand_packages::save(&self.store.pool, &package, verified, Some(&organ))
                    .await?
                {
                    return Err(invalid(
                        "This package version already contains different content.",
                    ));
                }
                Response::Saved { identity }
            }
        };
        Ok(ActionOutcome {
            data: Some(serde_json::to_value(response).map_err(|error| invalid(error.to_string()))?),
            ..Default::default()
        })
    }

    pub(crate) async fn require_package_contact(&self, organ: &str) -> Result<(), EngineError> {
        let contact = store::organs::contact(&self.store.pool, organ).await?;
        if !contact.is_some_and(|contact| contact.trust == "known") {
            return Err(EngineError::Forbidden(
                "Package exchange requires an existing known Organ contact.".into(),
            ));
        }
        Ok(())
    }

    async fn remote_packages(&self, organ: &str, query: Query) -> Result<Response, EngineError> {
        self.require_package_contact(organ).await?;
        if matches!(query, Query::List { offset } if offset > 100_000) {
            return Err(invalid("Package catalogue offset exceeds its limit."));
        }
        let response = self
            .transport_for("browse public Sand packages")?
            .sand_packages(organ, query.clone())
            .await?;
        self.require_package_contact(organ).await?;
        match response {
            Response::Catalogue { mut entries, next } if matches!(query, Query::List { .. }) => {
                if entries.len() > model::PAGE_SIZE as usize
                    || entries
                        .iter()
                        .any(|entry| entry.validate().is_err() || !entry.public)
                {
                    return Err(invalid("Invalid package catalogue."));
                }
                let Query::List { offset } = query else {
                    unreachable!()
                };
                if next.is_some_and(|next| next != offset + model::PAGE_SIZE || next > 100_000) {
                    return Err(invalid("Invalid package catalogue page."));
                }
                for entry in &mut entries {
                    entry.origin_verified = false;
                }
                Ok(Response::Catalogue { entries, next })
            }
            Response::Package { package, .. } => {
                let Query::Inspect { identity } = query else {
                    return Err(invalid("Invalid package response."));
                };
                package.validate().map_err(invalid)?;
                if package.manifest.identity != identity || !signature_valid(&package)? {
                    return Err(invalid("Package identity or signature does not match."));
                }
                let origin_verified = self.package_origin_verified(&package).await?;
                Ok(Response::Package {
                    package,
                    public: true,
                    origin_verified,
                })
            }
            _ => Err(invalid("Invalid package response.")),
        }
    }

    async fn package_catalogue(
        &self,
        public_only: bool,
        offset: u32,
    ) -> Result<Response, EngineError> {
        if offset > 100_000 {
            return Err(invalid("Package catalogue offset exceeds its limit."));
        }
        let mut entries = store::sand_packages::list(&self.store.pool, public_only, offset).await?;
        let next = (entries.len() > model::PAGE_SIZE as usize
            && offset + model::PAGE_SIZE <= 100_000)
            .then_some(offset + model::PAGE_SIZE);
        entries.truncate(model::PAGE_SIZE as usize);
        Ok(Response::Catalogue { entries, next })
    }

    pub async fn public_sand_packages(
        &self,
        organ: &str,
        query: Query,
    ) -> Result<Response, EngineError> {
        self.require_package_contact(organ).await?;
        let hidden = store::visibility::hidden_from_organ(&self.store.pool, organ).await?;
        match query {
            Query::List { offset } => {
                let Response::Catalogue { mut entries, next } =
                    self.package_catalogue(true, offset).await?
                else {
                    unreachable!()
                };
                entries.retain(|entry| !hidden.contains(&entry.identity.id));
                Ok(Response::Catalogue { entries, next })
            }
            Query::Inspect { identity } => {
                identity.validate().map_err(invalid)?;
                if hidden.contains(&identity.id) {
                    return Err(invalid("Package is unavailable."));
                }
                let stored = store::sand_packages::get(&self.store.pool, &identity, true)
                    .await?
                    .ok_or_else(|| invalid("Package is unavailable."))?;
                Ok(Response::Package {
                    package: stored.package,
                    public: true,
                    origin_verified: stored.origin_verified,
                })
            }
        }
    }

    async fn snapshot_package(
        &self,
        record: &str,
        kind: model::Kind,
        licenses: Vec<model::License>,
        credits: Vec<String>,
    ) -> Result<Package, EngineError> {
        let row = store::records::get(&self.store.pool, record)
            .await?
            .filter(|row| row.kind == "sand")
            .ok_or_else(|| invalid("Choose a saved Sand or Castle component."))?;
        let value: serde_json::Value =
            serde_json::from_str(&row.body).map_err(|_| invalid("Invalid component document."))?;
        let execution = if value["format"] == nucleus::canvas::Document::FORMAT
            || (value["format"] == nucleus::component::composition::FORMAT
                && value.get("composition").is_some())
        {
            model::DECLARATIVE
        } else if value["format"] == nucleus::component::composition::FORMAT
            && value.get("castle").is_some()
        {
            model::DESKTOP_LAYOUT
        } else {
            return Err(invalid("This component format cannot be packaged yet."));
        };
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ."))?;
        if row.organ_uid.as_deref() != Some(&local.uid)
            || store::replica::root_of(&self.store.pool, &row.uid)
                .await?
                .is_some()
        {
            return Err(invalid(
                "Only local component Records can create a new package identity.",
            ));
        }
        let organ_signer = self.organ_signer.lock().await.clone();
        let signer = match organ_signer {
            Some(signer) => Some(signer),
            None => self.signer.lock().await.clone(),
        }
        .filter(|signer| signer.actor_uid == local.uid)
        .ok_or_else(|| invalid("This Cell needs its Organ signing key before saving a package."))?;
        let version = u32::try_from(
            store::sand_packages::next_version(&self.store.pool, &local.uid, &row.uid).await?,
        )
        .map_err(|_| invalid("Package version limit reached."))?;
        let permissions = model::required_permissions(&row.body).map_err(invalid)?;
        let author = store::facts::creator_uid(&self.store.pool, &row.uid)
            .await?
            .unwrap_or_else(|| local.uid.clone());
        let mut package = Package {
            format: model::FORMAT.into(),
            manifest: Manifest {
                identity: Identity {
                    origin: local.uid,
                    id: row.uid,
                    version,
                },
                name: row.head,
                kind,
                author,
                execution: execution.into(),
                permissions,
                licenses,
                credits,
                key_id: signer.key_id.clone(),
                public_key: signer.public_key_b64(),
            },
            payload: row.body,
            digest: String::new(),
            signature: String::new(),
        };
        package.digest = package.content_digest().map_err(invalid)?;
        package.signature = signer.sign_bytes(&package.signing_bytes().map_err(invalid)?);
        package.validate().map_err(invalid)?;
        Ok(package)
    }

    pub async fn package_origin_verified(&self, package: &Package) -> Result<bool, EngineError> {
        if !signature_valid(package)? {
            return Ok(false);
        }
        let manifest = &package.manifest;
        if !store::records::get(&self.store.pool, &manifest.identity.origin)
            .await?
            .is_some_and(|origin| origin.kind == "organ")
        {
            return Ok(false);
        }
        if store::roster::is_revoked(
            &self.store.pool,
            &manifest.identity.origin,
            &manifest.public_key,
        )
        .await?
        {
            return Ok(false);
        }
        let keys: Vec<String> = store::sqlx::query_scalar("SELECT public_key FROM identity_key WHERE actor_uid = ? UNION SELECT json_extract(member.value, '$.operational_key') FROM organ_roster AS roster, json_each(roster.payload, '$.cells') AS member WHERE roster.organ_uid = ?")
            .bind(&manifest.identity.origin).bind(&manifest.identity.origin).fetch_all(&self.store.pool).await?;
        Ok(keys.contains(&manifest.public_key))
    }
}

fn signature_valid(package: &Package) -> Result<bool, EngineError> {
    package.validate().map_err(invalid)?;
    Ok(crate::roster::verify_with(
        &package.manifest.public_key,
        &package.signing_bytes().map_err(invalid)?,
        &package.signature,
    ))
}
