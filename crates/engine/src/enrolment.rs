//! Joining an existing Organ as a new Cell (Ontology §11, cluster C3).
//!
//! Enrolling a device is PAIRING WITH YOURSELF, and it earns its own flow. The
//! server half has existed since migration 0044 — a single-use, short-lived
//! token, redeemed on `lince/thread/2`, which signs the presenting Cell into
//! the roster with `full_capabilities()`. This is the half that was missing:
//! the client, and what it means for a device to stop being its own Organ and
//! become a member of another.
//!
//! # The order is the security property
//!
//! The joining Cell does NOT adopt the identity first and ask afterwards. It
//! mints the operational key it intends to be known by, sends `Enrol`, and
//! only rewrites its local identity once the roster comes back naming it. A
//! refused enrolment therefore leaves the device exactly as it was, rather
//! than having destroyed an Organ on the strength of a code that turned out
//! not to work.
//!
//! That ordering also settles a C1 leftover. `trust::set_organ_signer` binds
//! the operational key's `actor_uid` to the ORGAN, so a key minted before the
//! swap would be filed under an Organ that no longer exists locally, and peers
//! resolving keys by Organ would fail to verify anything it signed. The key is
//! therefore minted for the JOINED uid from the start — same secret, so the
//! key file on disk stays correct across reboots — and bound after the swap.

use crate::error::EngineError;
use crate::pairing::EnrolmentInvite;
use crate::roster::{ROOT_KEY_ID, RosterOutcome, SignedRoster};
use crate::trust::Signer;

/// Marks the identity as in flux for as long as it is held, INCLUDING on the
/// error paths — a `?` in the middle of the swap must not leave the Cell
/// permanently unable to answer for itself.
struct Joining<'a>(&'a crate::Engine);

impl<'a> Joining<'a> {
    fn begin(engine: &'a crate::Engine) -> Joining<'a> {
        engine
            .joining
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Joining(engine)
    }
}

impl Drop for Joining<'_> {
    fn drop(&mut self) {
        self.0
            .joining
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// The one thing an Action needs from the transport to enrol this Cell:
/// somewhere to send the code.
///
/// A narrow trait rather than the Engine holding a `Wire`, because Wire holds
/// an `Arc<Engine>` and the reverse would be a cycle. Installed from above the
/// way `Wire` takes its live and transfer handlers, and held WEAKLY so the
/// Engine never keeps an endpoint alive.
///
/// It exists because a feature nobody can reach is not a feature: without this
/// the enrolment client is callable from a test and from nowhere else, and
/// "add this device to my Organ" cannot be done from the running app.
#[async_trait::async_trait]
pub trait CellTransport: Send + Sync {
    async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError>;
    /// Compare logs with a contact without moving any ops. `None` means they
    /// could not be reached, which is not a disagreement.
    async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<crate::wire::AuditAgreement>, EngineError>;
    /// Ask a Cell whether it is carrying mail for US, at `node_id`.
    ///
    /// Three answers rather than a `Result`, because publishing a pickup point
    /// turns on the difference: a REFUSAL is a real answer and means do not
    /// publish, while no answer at all means "not right now", which is the
    /// ordinary state of most machines and no reason to conclude anything.
    async fn carrier_probe(&self, node_id: &str) -> crate::wire::CarrierProbe;
    /// Collect from every pickup point we published, right now.
    async fn collect_mail_now(&self) -> Result<usize, EngineError>;
    /// Run one whole sync pass now, rather than at the next tick.
    async fn sync_now(&self) -> Result<usize, EngineError>;
    /// Ask the Cell at `node_id` to carry our mail. Their operator answers
    /// later; this only delivers the ask.
    async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError>;
    /// Spend an invite code with the box that issued it.
    async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError>;
}

impl crate::Engine {
    /// Install the transport this Cell enrols through.
    pub fn set_enroller(&self, enroller: std::sync::Weak<dyn CellTransport>) {
        *self.enroller.lock().expect("enroller") = Some(enroller);
    }

    /// Join an Organ from a pasted or scanned enrolment code.
    pub async fn join_from_code(&self, code: &str) -> Result<SignedRoster, EngineError> {
        let invite = EnrolmentInvite::decode(code)?;
        let enroller = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot reach the device that \
                     showed the code."
                        .into(),
                )
            })?;
        enroller.enrol(&invite).await
    }

    /// Compare logs with a contact and hand back what disagrees.
    pub async fn audit_contact(
        &self,
        contact_organ: &str,
    ) -> Result<Option<crate::wire::AuditAgreement>, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot compare logs with \
                     anyone."
                        .into(),
                )
            })?;
        transport.audit_against(contact_organ).await
    }

    /// Ask a Cell whether it carries mail for us. `Unreachable` when this Cell
    /// has no endpoint at all, which is the same answer from the caller's side.
    pub async fn carrier_probe(&self, node_id: &str) -> crate::wire::CarrierProbe {
        // Bound to a local BEFORE the await: holding the guard across it would
        // make every caller's future non-Send.
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade());
        match transport {
            Some(transport) => transport.carrier_probe(node_id).await,
            None => crate::wire::CarrierProbe::Unreachable,
        }
    }

    /// Collect our mail from every published pickup point immediately.
    pub async fn collect_mail_now(&self) -> Result<usize, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot reach a mailbox.".into(),
                )
            })?;
        transport.collect_mail_now().await
    }

    /// Run one whole sync pass now.
    ///
    /// The pass, not a special path: whatever it does for a contact when it
    /// runs on its own timer is exactly what a button labelled "now" should
    /// do, and anything that bypassed it would be a second implementation of
    /// delivery that nothing else exercises.
    pub async fn sync_now(&self) -> Result<usize, EngineError> {
        let transport = self
            .enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(
                    "this Cell has no network endpoint, so it cannot sync with anyone.".into(),
                )
            })?;
        transport.sync_now().await
    }

    /// Ask a Cell to carry our mail. Delivers the ask and nothing more —
    /// whether they will is their operator's answer, given later.
    pub async fn ask_carrier(&self, node_id: &str) -> Result<(), EngineError> {
        self.transport_for("ask anyone to carry your mail")?
            .ask_to_be_carried(node_id)
            .await
    }

    /// Spend a mailbox invite code. Returns the label and quota it carried.
    pub async fn redeem_carry_code(&self, code: &str) -> Result<(String, i64), EngineError> {
        let invite = crate::pairing::MailboxInviteCode::decode(code)?;
        self.transport_for("reach the box that issued that code")?
            .redeem_mailbox_invite(&invite.node_id, &invite.token)
            .await
    }

    /// The installed transport, or a refusal that says what could not be done
    /// rather than that a pointer was missing.
    fn transport_for(&self, what: &str) -> Result<std::sync::Arc<dyn CellTransport>, EngineError> {
        self.enroller
            .lock()
            .expect("enroller")
            .clone()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                EngineError::Consequence(format!(
                    "this Cell has no network endpoint, so it cannot {what}."
                ))
            })
    }

    /// Whether this Cell's identity is being rewritten right now.
    pub(crate) fn identity_in_flux(&self) -> bool {
        self.joining.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether this Cell could join another Organ right now, and why not if it
    /// cannot. Checked BEFORE dialing so the failure arrives without having
    /// spent a single-use token.
    pub async fn may_enrol(&self) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        if self.roster_of(&organ.uid).await?.is_some() {
            // A roster means this Cell has already published an identity that
            // contacts may hold. Joining another one would strand them.
            return Err(EngineError::Consequence(
                "this Cell already has a published identity of its own. Joining another \
                 Organ would abandon it, and every contact holding its key with it."
                    .into(),
            ));
        }
        // The SAME condition `adopt_identity` enforces, asked here as well and
        // deliberately not only there. The swap runs after the other Cell has
        // redeemed the token and signed a new roster, so discovering
        // ineligibility at that point costs a single-use code and leaves a
        // roster naming a device that never joined.
        if store::organs::holds_own_records(&self.store.pool).await? {
            return Err(EngineError::Consequence(
                "this Cell already holds Records of its own, so joining another Organ \
                 would have to merge two identities. Enrol a device that has not been \
                 used yet."
                    .into(),
            ));
        }
        Ok(())
    }

    /// Become a Cell of `invite.organ_uid`, given the roster that names us.
    ///
    /// Every check is done here rather than trusted from the connection: the
    /// roster must be for the Organ we were invited to, signed by the root key
    /// the invite carried, and it must actually list this Cell. A peer that
    /// answers `Enrol` with somebody else's roster gets nothing.
    pub async fn join_organ(
        &self,
        invite: &EnrolmentInvite,
        signed: &SignedRoster,
        operational: Signer,
    ) -> Result<(), EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        if signed.roster.organ_uid != invite.organ_uid {
            return Err(EngineError::Consequence(
                "the roster returned is for a different Organ than the code offered".into(),
            ));
        }
        if signed.roster.root_key != invite.root_key {
            return Err(EngineError::Consequence(
                "the roster returned is signed by a different root key than the code \
                 carried"
                    .into(),
            ));
        }
        if !signed
            .roster
            .cells
            .iter()
            .any(|member| member.cell_uid == cell.uid)
        {
            return Err(EngineError::Consequence(
                "the roster returned does not list this device".into(),
            ));
        }

        let base_url = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.base_url)
            .unwrap_or_default();
        // From here until the roster is stored, this Cell's local Organ is the
        // joined one with NO roster — and "no roster" reads elsewhere as "this
        // single Cell is the whole Organ, so it may represent it". An inbound
        // knock landing in that window would be bound on the spot by a Cell
        // about to learn it holds no such capability, which is exactly what a
        // relay enrolling as a front door is. Ordering cannot close it (the
        // roster's mirror needs the joined Organ Record to exist first), so
        // the window is announced.
        let _joining = Joining::begin(self);
        // The swap. Refuses on a Cell that already holds Records of its own.
        store::organs::adopt_identity(&self.store.pool, &invite.organ_uid, &base_url).await?;
        // Trust-on-first-use, over the same unrelayable visual channel pairing
        // uses. Adopted BEFORE the roster, because `adopt_roster` checks that
        // the signing key chains from one we already hold — and this is the
        // key it must chain from.
        crate::trust::adopt_key(
            &self.store,
            &invite.organ_uid,
            ROOT_KEY_ID,
            &invite.root_key,
        )
        .await?;
        match self.adopt_roster(signed).await? {
            RosterOutcome::Accepted | RosterOutcome::NotNewer => {}
            RosterOutcome::Expired => {
                return Err(EngineError::Consequence(
                    "the roster returned has already expired".into(),
                ));
            }
            RosterOutcome::Refused => {
                return Err(EngineError::Consequence(
                    "the roster returned is not signed by the root key the code carried".into(),
                ));
            }
        }
        // Last, and only now valid: the signer's `actor_uid` is the joined
        // Organ, which is the uid `set_organ_signer` insists the local Organ
        // must have.
        self.set_organ_signer(operational).await?;
        tracing::info!(
            organ = %invite.organ_uid,
            cell = %cell.uid,
            members = signed.roster.cells.len(),
            "this Cell is now a member of an existing Organ"
        );
        Ok(())
    }

    /// The operational key this Cell will present, minted for the Organ it is
    /// about to join.
    ///
    /// Reuses the SECRET already on this Cell when there is one, so the key
    /// file on disk keeps matching what the roster records after a reboot.
    /// Only the `actor_uid` it is filed under changes.
    pub async fn operational_key_for(&self, organ_uid: &str) -> Result<Signer, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        // Filed under THIS CELL's key id, which is what lets a second device
        // of one Organ hold a key of its own at all.
        let key_id = crate::roster::cell_key_id(&cell.uid);
        Ok(match self.organ_signer.lock().await.as_ref() {
            Some(existing) => Signer::from_bytes(organ_uid, &key_id, existing.secret_bytes()),
            None => Signer::generate(organ_uid, &key_id),
        })
    }
}
