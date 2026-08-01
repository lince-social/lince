//! Lince engine (blueprint Part 0): one organism, one write path.
//!
//! - `append`/`append_user`: the ONLY code that mutates `record.quantity`
//!   (fact + cache bump in one transaction), followed by a synchronous,
//!   iteration-capped reaction — deterministic and DST-friendly.
//! - `fire_due_rules(now)`: applies every date a declared rule expected.
//! - `run_due_effects`: executes queued shell/notify effects OUTSIDE evaluation.

pub mod action_intent;
pub mod actions;
pub mod append;
pub mod checkpoint;
#[allow(dead_code)]
pub mod communication;
pub mod effects;
pub mod error;
pub mod expiry;
pub mod file_sync;
pub mod imagination;
pub mod karma_control;
pub mod karma_grants;
pub mod karma_runtime;
pub mod karma_timezone;
pub mod senses;
pub mod signals;
pub mod sync;
pub mod transfer;
pub mod transfer_delivery;
pub mod trust;

use chrono::{DateTime, TimeDelta, Utc};

use crate::actions::Action;

/// How far back a waking Cell looks for dates it slept through.
///
/// Bounded on purpose: "since the anchor" would have a rule declared a year ago
/// replay a year of dates the first time the daemon runs.
const RECURRENCE_CATCH_UP_DAYS: i64 = 60;

/// The most dates one rule may apply in a single tick.
///
/// A missed month is four occurrences; a millisecond rule asleep for a day is
/// tens of millions. The cap is what stops the second case from starving the
/// heartbeat, and it loses nothing — occurrences are derived, so the remainder
/// is still there on the next tick.
const MAX_AUTO_APPLIES_PER_RULE_PER_TICK: usize = 64;

/// How many rule evaluations one committed change may set off before the chain
/// is cut.
///
/// A rule that changes a Record another rule reads is the point of having
/// rules, and following that chain is what makes automation feel immediate
/// rather than delayed to the next beat. But a chain is also how a mistake
/// becomes a spin, so it is bounded. Nothing is lost when the bound is hit:
/// what did not run reactively still runs on the next heartbeat.
const MAX_REACTIONS_PER_CHANGE: usize = 256;

/// Run `work` marked as a rule firing, if it is not already inside one.
///
pub(crate) fn as_one_firing<F>(work: F) -> impl std::future::Future<Output = F::Output>
where
    F: std::future::Future,
{
    INSIDE_REACTION.scope((), work)
}

/// Whether this task is already inside a rule firing.
pub(crate) fn already_firing() -> bool {
    INSIDE_REACTION.try_with(|()| ()).is_ok()
}

tokio::task_local! {
    /// Whether this task is already inside a reaction.
    ///
    /// A rule fires through the ordinary write path, so the facts it commits
    /// come back round to the reactive path that fired it. Left alone that is
    /// unbounded *recursion* — each firing nests inside the last — and it
    /// overflows the stack long before any counter notices, because the counter
    /// is per call and every nested call starts it again.
    ///
    /// So only the outermost reaction runs the chain. It already follows every
    /// Record its rules move, through its own queue, which is the same work
    /// done iteratively instead of by nesting. Task-local rather than a field
    /// on the Engine because two requests handled at once are two independent
    /// chains, and a shared counter would have one silently mute the other.
    static INSIDE_REACTION: ();
}
use nucleus::{Cause, Fact, NewFact};
use std::sync::RwLock;
use store::Store;
use tokio::sync::{Mutex, broadcast, watch};

pub use error::EngineError;

pub struct Engine {
    pub store: Store,
    bus: broadcast::Sender<Fact>,
    pub(crate) signer: Mutex<Option<trust::Signer>>,
    pub(crate) organ_signer: Mutex<Option<trust::Signer>>,
    karma_deadline_changed: watch::Sender<u64>,
    karma_runtime_config: RwLock<Option<karma_runtime::KarmaDeadlineDirectorConfig>>,
}

impl Engine {
    pub async fn new(store: Store) -> Result<Engine, EngineError> {
        let (bus, _) = broadcast::channel(1024);
        let (karma_deadline_changed, _) = watch::channel(0);
        let engine = Engine {
            store,
            bus,
            signer: Mutex::new(None),
            organ_signer: Mutex::new(None),
            karma_deadline_changed,
            karma_runtime_config: RwLock::new(None),
        };
        Ok(engine)
    }

    pub async fn open_memory() -> Result<Engine, EngineError> {
        Self::new(Store::open_memory().await?).await
    }

    /// Subscribe to committed facts (blueprint 0.2 `fact_bus`).
    pub fn subscribe(&self) -> broadcast::Receiver<Fact> {
        self.bus.subscribe()
    }

    /// Wake the tickless Karma deadline runner after a committed activation,
    /// pause, parameter epoch, resource grant, or host timer capability change.
    pub fn notify_karma_deadline_change(&self) {
        self.karma_deadline_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    /// Install the immutable admission/provider snapshot used by typed Karma
    /// Frequency Actions. Starting the director and installing its control
    /// snapshot are explicit so boot code can finish dependency injection
    /// before either mutations or timers are accepted.
    pub fn install_karma_runtime_config(
        &self,
        config: karma_runtime::KarmaDeadlineDirectorConfig,
    ) -> Result<(), EngineError> {
        *self
            .karma_runtime_config
            .write()
            .map_err(|_| EngineError::Conflict {
                code: "karma_runtime_config_poisoned",
                message: "Karma runtime configuration lock is poisoned".to_string(),
            })? = Some(config);
        self.notify_karma_deadline_change();
        Ok(())
    }

    pub(crate) fn configured_karma_runtime(
        &self,
    ) -> Result<karma_runtime::KarmaDeadlineDirectorConfig, EngineError> {
        self.karma_runtime_config
            .read()
            .map_err(|_| EngineError::Conflict {
                code: "karma_runtime_config_poisoned",
                message: "Karma runtime configuration lock is poisoned".to_string(),
            })?
            .clone()
            .ok_or_else(|| EngineError::Conflict {
                code: "karma_runtime_unconfigured",
                message: "Karma Frequency activation requires an installed runtime configuration"
                    .to_string(),
            })
    }

    /// Append a fact and run the reactive Karma cascade. Returns every fact
    /// committed (the trigger plus all rule firings), in order. Every fact is
    /// signed when a signer is installed (Trust, XI).
    pub async fn append(&self, new: NewFact, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        if let Some(fact) = append::append_one(&self.store, new, now, signer.as_ref()).await? {
            return self.observe_committed_fact(fact, now).await;
        }
        Ok(Vec::new())
    }

    /// Publish a Fact that was committed inside a larger semantic transaction,
    /// then run the same reactive cascade as the normal append path.
    pub(crate) async fn observe_committed_fact(
        &self,
        fact: Fact,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let _ = self.bus.send(fact.clone());
        let changed = vec![fact.record_uid.clone()];
        let mut committed = vec![fact];
        committed.extend(self.react_to(changed, now).await?);
        Ok(committed)
    }

    /// Let every rule that reads one of these Records look at it now.
    ///
    /// This is the other half of "a rule fires itself". The heartbeat covers
    /// *time* passing; this covers the *world* changing — "when stock drops
    /// below three" has to mean the moment it drops, not up to a minute later.
    ///
    /// A change makes a rule **look**; its cadence still decides whether it may
    /// **act**. Both paths therefore go through the same occurrence: the latest
    /// date the rule has produced. That single decision buys three things at
    /// once. A reactive firing and a scheduled one are the same Action with the
    /// same idempotency key, so they cannot double-apply each other. A rule can
    /// act at most once per cadence period, which is what a debounce was, only
    /// now it is declared in the same place as everything else about the rule.
    /// And a gate that blocked earlier in the period does not spend the date, so
    /// the rule fires the instant the world makes its condition true.
    pub(crate) async fn react_to(
        &self,
        changed: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        if already_firing() {
            // A rule firing inside a reaction: the chain is already being
            // followed by the reaction that started it, and its facts reach
            // that queue by return value.
            return Ok(Vec::new());
        }
        as_one_firing(self.react_to_inner(changed, now)).await
    }

    async fn react_to_inner(
        &self,
        changed: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let mut committed = Vec::new();
        // Only a rule with an *if* can be reactive: an unconditional rule reads
        // nothing, so no change can concern it, and its dates are the whole of
        // what it responds to.
        let watchers: Vec<store::recurrence::Recurrence> =
            store::recurrence::all(&self.store.pool)
                .await?
                .into_iter()
                .filter(|rule| rule.condition.is_some() && !rule.is_paused())
                .collect();
        if watchers.is_empty() {
            return Ok(committed);
        }

        let mut pending: std::collections::VecDeque<String> = changed.into();
        let mut steps = 0usize;
        while let Some(record_uid) = pending.pop_front() {
            for rule in &watchers {
                steps += 1;
                if steps > MAX_REACTIONS_PER_CHANGE {
                    return Ok(committed);
                }
                if !self.rule_reads(rule, &record_uid).await? {
                    continue;
                }
                // The period the rule is currently in. A rule whose first date
                // is still ahead has nothing to act on yet.
                let Ok(anchor) = actions::parse_instant_field(&rule.anchor_at) else {
                    continue;
                };
                let Some(edge) = now.checked_add_signed(TimeDelta::milliseconds(1)) else {
                    continue;
                };
                let Ok(Some(due)) = rule.cadence.preceding(anchor, edge) else {
                    continue;
                };
                let outcome = Box::pin(self
                    .act_at(
                        Action::ApplyRecurrenceOccurrence {
                            recurrence: rule.uid.clone(),
                            due_at: due.to_rfc3339(),
                            amount: None,
                            note: None,
                        },
                        // Nobody pressed this either. The declaration is the
                        // authority, exactly as it is for a date falling due.
                        None,
                        now,
                    ))
                    .await;
                // One rule that cannot run must not stop the others, and must
                // not roll back the change that woke it.
                let Ok(outcome) = outcome else { continue };
                for fact in &outcome.facts {
                    let _ = self.bus.send(fact.clone());
                    // Follow what this rule moved, so a chain of rules settles
                    // on one change rather than one per beat.
                    pending.push_back(fact.record_uid.clone());
                }
                committed.extend(outcome.facts);
            }
        }
        Ok(committed)
    }

    /// Whether this rule's condition reads that Record.
    ///
    /// Resolved through the same names the condition was written in, so a rule
    /// watching `@apples.stock` follows the slug to whatever Record holds it.
    async fn rule_reads(
        &self,
        rule: &store::recurrence::Recurrence,
        record_uid: &str,
    ) -> Result<bool, EngineError> {
        let Some(condition) = rule.condition.as_ref() else {
            return Ok(false);
        };
        let Ok(parsed) = nucleus::karma::Condition::parse(&condition.source) else {
            // Refused at write time; if one survives, it simply watches nothing.
            return Ok(false);
        };
        for token in parsed.reads() {
            // A rhythm is not a reading of the world — it is a reading of the
            // calendar. Waking a rule because the Record carrying a schedule
            // moved would fire it off-schedule, which is the one thing the
            // cadence is there to prevent.
            if token.func == "freq" {
                continue;
            }
            let name = token.slug.trim_start_matches('@');
            if let Some(record) = store::records::resolve(&self.store.pool, name).await?
                && record.uid == record_uid
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Publish semantic transaction evidence without running Karma. Transfer
    /// revisions use this until the dedicated Transfer/Karma phase defines
    /// which signed term changes may drive recommendations or automation.
    pub(crate) fn publish_committed_fact(&self, fact: Fact) -> Vec<Fact> {
        let _ = self.bus.send(fact.clone());
        vec![fact]
    }

    /// Convenience: user edits a quantity by delta, clocked now.
    pub async fn append_user(
        &self,
        record_uid: &str,
        delta: f64,
    ) -> Result<Vec<Fact>, EngineError> {
        self.append(
            NewFact::quantity_f64(record_uid, delta, Cause::user_edit()),
            Utc::now(),
        )
        .await
    }

    /// Apply every date a declared rule expected that nobody answered.
    ///
    /// This is what makes a rule automatic. Declaring "every Monday, set this
    /// back to -1" *is* the authorization; asking the person to confirm it again
    /// every Monday would mean the declaration said nothing. So the wheel
    /// presses apply on their behalf, through the **same Action** a person uses
    /// — an automatic change stays auditable by identical means to a manual one,
    /// and never acquires a private write path.
    ///
    /// Only consequences that touch the author's own Records travel this way.
    /// Anything leaving the Cell still fails closed and still needs a grant.
    pub async fn fire_due_rules(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        // A rule's own facts must not start a reaction *while the rule is still
        // applying*: the entry that marks the date done is not committed yet, so
        // a reaction firing here would find the date unspent and apply it a
        // second time. Chains are still followed — just afterwards, from the
        // outside, where the marker is visible and idempotency holds.
        let facts = self.fire_due_rules_inner(now).await?;
        let mut changed: Vec<String> = facts.iter().map(|f| f.record_uid.clone()).collect();
        changed.sort();
        changed.dedup();
        let mut committed = facts;
        committed.extend(self.react_to(changed, now).await?);
        Ok(committed)
    }

    async fn fire_due_rules_inner(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut committed = Vec::new();
        // How far back a waking Cell looks for dates it slept through. Bounded
        // because "since the anchor" would mean a rule declared last year
        // replays a year on first boot.
        let from = now - TimeDelta::days(RECURRENCE_CATCH_UP_DAYS);
        let Some(to) = now.checked_add_signed(TimeDelta::milliseconds(1)) else {
            return Ok(committed);
        };
        for rule in store::recurrence::all(&self.store.pool).await? {
            // A paused rule offers nothing, including the dates that fell due
            // before it was paused. Pausing means "stop acting for me".
            if rule.is_paused() {
                continue;
            }
            let Ok(derived) =
                store::recurrence::occurrences(&self.store.pool, &rule, from, to, now).await
            else {
                // One unreadable rule must not stop every other rule's dates.
                continue;
            };
            let mut applied = 0usize;
            for occurrence in derived.dates {
                if occurrence.state != store::recurrence::OccurrenceState::Due {
                    continue;
                }
                // The danger is not a missed month; it is a millisecond rule
                // asleep for a day. The remainder is not lost — it is still
                // derived, and the next tick takes the next slice.
                if applied >= MAX_AUTO_APPLIES_PER_RULE_PER_TICK {
                    break;
                }
                applied += 1;
                // Idempotent by construction: apply is keyed on rule + date and
                // refuses a repeat before running any consequence, so a person
                // who already pressed this date loses nothing to the wheel.
                let outcome = self
                    .act_at(
                        Action::ApplyRecurrenceOccurrence {
                            recurrence: rule.uid.clone(),
                            due_at: occurrence.due_at.to_rfc3339(),
                            // The rule's own figure. An override is a judgement
                            // about one date, which is exactly what nobody is
                            // here to make.
                            amount: None,
                            note: None,
                        },
                        // No actor: nobody pressed this. The rule's declaration
                        // is the authority, and saying a person did it would be
                        // a false attribution in the Ledger.
                        None,
                        now,
                    )
                    .await;
                match outcome {
                    Ok(outcome) => {
                        for fact in &outcome.facts {
                            let _ = self.bus.send(fact.clone());
                        }
                        committed.extend(outcome.facts);
                    }
                    // A rule whose consequence is refused (a deleted concept, a
                    // vanished Record) must not stop the wheel for every other
                    // rule. It stays due and says so on the surface.
                    Err(_) => continue,
                }
            }
        }
        Ok(committed)
    }

    /// Run queued effects (blueprint VI: Effects never run inside evaluation).
    /// One beat of the organism (blueprint 0.2, callable form): promises
    /// expire, timers fire, signals sample, effects run. The daemon wraps this
    /// in an interval; DST calls it with a virtual clock.
    pub async fn heartbeat(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        // Expiry first: rules evaluated by the tick must see true promise states.
        let mut facts = self.expire_promises(now).await?;
        facts.extend(self.expire_due_transfer_invitations(now).await?);
        facts.extend(self.expire_decisions(now).await?);
        // Declared rules act for the person who declared them. After the wheel,
        // so a rule reading a Record a Frequency just moved sees the new value
        // on this beat rather than the next one.
        facts.extend(self.fire_due_rules(now).await?);
        facts.extend(self.sample_due_signals(now).await?);
        self.run_due_effects().await?;
        // Senses (X): match open promises against the discovery cache; drafts
        // land in the Decision Queue.
        self.senses_pass().await?;
        // Imagination (XII): projected threshold crossings become decisions.
        self.crossings_pass(now).await?;
        Ok(facts)
    }

    /// The daemon loop: heartbeat every `period_secs` until the handle drops.
    pub fn run(self: std::sync::Arc<Self>, period_secs: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(period_secs.max(1)));
            loop {
                interval.tick().await;
                let _ = self.heartbeat(Utc::now()).await;
            }
        })
    }
}
