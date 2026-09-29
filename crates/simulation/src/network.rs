use engine::roster::{CellEntry, ROOT_KEY_ID, full_capabilities};
use engine::trust::Signer;
use nucleus::karma::{ReferenceKind, TypedUid};
use nucleus::simulation::{Cause, Observation};

use crate::Result;
use crate::world::{World, derived_secret};

impl World {
    pub(crate) async fn select_person(
        &mut self,
        name: &str,
        person: &str,
        cause: Cause,
    ) -> Result<()> {
        let person = self.resolve_reference(person);
        let node = self.nodes.get_mut(name).ok_or("missing Cell")?;
        node.set_time(self.now_ms)?;
        let record = store::records::get(&node.engine().store.pool, &person)
            .await?
            .ok_or("missing Person")?;
        let organ = store::organs::local(&node.engine().store.pool)
            .await?
            .ok_or("missing Organ")?;
        if record.kind != "person" || record.organ_uid.as_deref() != Some(&organ.uid) {
            return Err("simulated Person key must belong to this Organ".into());
        }
        let signer = Signer::from_bytes(
            &record.uid,
            "simulation-person",
            derived_secret(self.scenario.seed, &record.uid, "person"),
        );
        node.execution
            .scope(node.engine().set_signer(signer.clone()))
            .await?;
        node.person_key = Some(signer);
        self.observe(name, cause).await
    }
    pub(crate) async fn publish_identity(&self, name: &str) -> Result<Signer> {
        let node = &self.nodes[name];
        node.set_time(self.now_ms)?;
        node.execution
            .scope(async {
                let engine = node.engine();
                let organ = store::organs::local(&engine.store.pool)
                    .await?
                    .ok_or("missing Organ")?;
                let root = Signer::from_bytes(
                    &organ.uid,
                    ROOT_KEY_ID,
                    derived_secret(self.scenario.seed, &organ.uid, "root"),
                );
                if engine.roster_of(&organ.uid).await?.is_none() {
                    let cell = store::cells::local(&engine.store.pool)
                        .await?
                        .ok_or("missing Cell")?;
                    let operational = engine.operational_key_for(&organ.uid).await?;
                    engine.set_organ_signer(operational.clone()).await?;
                    engine.publish_root_key(&root).await?;
                    engine
                        .publish_roster(
                            &root,
                            vec![CellEntry {
                                cell_uid: cell.uid,
                                node_id: nucleus::fact::sha256_hex(&derived_secret(
                                    self.scenario.seed,
                                    name,
                                    "node",
                                )),
                                label: cell.label,
                                operational_key: operational.public_key_b64(),
                                sealing_key: None,
                                front_door: false,
                                capabilities: full_capabilities(),
                            }],
                        )
                        .await?;
                }
                Ok(root)
            })
            .await
    }

    pub(crate) async fn enrol(&mut self, name: &str, host: &str, cause: Cause) -> Result<()> {
        let root = self.publish_identity(host).await?;
        let node = &self.nodes[name];
        node.set_time(self.now_ms)?;
        let host_node = &self.nodes[host];
        let cell = store::cells::local(&node.engine().store.pool)
            .await?
            .ok_or("missing Cell")?;
        node.execution.scope(node.engine().may_enrol()).await?;
        let operational = node
            .execution
            .scope(node.engine().operational_key_for(&root.actor_uid))
            .await?;
        let token = host_node
            .execution
            .scope(host_node.engine().issue_enrolment_token())
            .await?;
        let invite = engine::pairing::EnrolmentInvite {
            node_id: nucleus::fact::sha256_hex(&derived_secret(self.scenario.seed, host, "node")),
            organ_uid: root.actor_uid.clone(),
            root_key: root.public_key_b64(),
            token: token.clone(),
            addrs: Vec::new(),
        };
        let roster = host_node
            .execution
            .scope(host_node.engine().redeem_enrolment(
                &root,
                &token,
                CellEntry {
                    cell_uid: cell.uid,
                    node_id: nucleus::fact::sha256_hex(&derived_secret(
                        self.scenario.seed,
                        name,
                        "node",
                    )),
                    label: cell.label,
                    operational_key: operational.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: full_capabilities(),
                },
            ))
            .await?;
        node.execution
            .scope(
                node.engine()
                    .join_organ(&invite, &roster, operational.clone()),
            )
            .await?;
        node.execution
            .scope(node.engine().set_signer(operational))
            .await?;
        self.captured
            .insert(format!("cell:{name}:organ"), root.actor_uid.clone());
        self.nodes.get_mut(name).ok_or("missing Cell")?.person_key = None;
        self.emit(
            name,
            cause.clone(),
            Observation::Enrolled {
                host: host.into(),
                organ: TypedUid::new(ReferenceKind::Organ, root.actor_uid)?,
                roster_version: roster.roster.version.try_into()?,
            },
        )?;
        self.observe(host, cause.clone()).await?;
        self.tick(name, cause).await
    }

    pub(crate) async fn discover(&mut self, name: &str, peer: &str, cause: Cause) -> Result<()> {
        let sender = &self.nodes[peer];
        sender.set_time(self.now_ms)?;
        let intro = sender
            .execution
            .scope(sender.engine().introduction())
            .await?;
        let receiver = &self.nodes[name];
        receiver.set_time(self.now_ms)?;
        receiver
            .execution
            .scope(receiver.engine().adopt_introduction(&intro, 1))
            .await?;
        self.emit(
            name,
            cause.clone(),
            Observation::Discovered {
                peer: peer.into(),
                organ: TypedUid::new(ReferenceKind::Organ, intro.organ_uid)?,
            },
        )?;
        self.observe(name, cause).await
    }
}
