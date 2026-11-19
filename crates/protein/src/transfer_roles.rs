use std::{cell::OnceCell, collections::HashSet};
use store::transfers::{
    AgreementPromiseReadinessRow as Promise, TransferAgreementReadinessInput as Input,
};

struct People<'a> {
    input: &'a Input,
    accepted: OnceCell<HashSet<&'a str>>,
    coalition: OnceCell<HashSet<&'a str>>,
    fallback: OnceCell<Vec<&'a str>>,
}

impl<'a> People<'a> {
    fn new(input: &'a Input) -> Self {
        Self {
            input,
            accepted: OnceCell::new(),
            coalition: OnceCell::new(),
            fallback: OnceCell::new(),
        }
    }

    fn accepted(&self) -> &HashSet<&'a str> {
        self.accepted.get_or_init(|| {
            let _people = nucleus::performance::Span::new("protein-transfer-role-people");
            self.input
                .parties
                .iter()
                .map(|party| party.person_uid.as_str())
                .collect()
        })
    }

    fn coalition(&self) -> Option<&HashSet<&'a str>> {
        let coalition = self.input.coalition.as_ref()?;
        Some(self.coalition.get_or_init(|| {
            let _people = nucleus::performance::Span::new("protein-transfer-role-people");
            let parties = coalition
                .party_uids
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>();
            self.input
                .parties
                .iter()
                .filter(|party| parties.contains(party.party_uid.as_str()))
                .map(|party| party.person_uid.as_str())
                .collect()
        }))
    }

    fn eligible(&self, actor: &str, person: Option<&str>) -> bool {
        person.is_some_and(|person| {
            person != actor
                && self
                    .coalition()
                    .is_none_or(|members| members.contains(person))
        })
    }

    fn fallback(&self) -> &[&'a str] {
        let accepted = self.accepted();
        let coalition = self.coalition();
        self.fallback.get_or_init(|| {
            let _people = nucleus::performance::Span::new("protein-transfer-role-people");
            accepted
                .iter()
                .copied()
                .filter(|person| coalition.is_none_or(|members| members.contains(person)))
                .collect()
        })
    }
}

struct Roles<'a> {
    opposite: Option<&'a str>,
    giver: &'a str,
    receiver: &'a str,
}

struct Prepared<'a> {
    paths: store::transfer_agreement::Paths<'a>,
    people: People<'a>,
}

impl<'a> Prepared<'a> {
    fn new(input: &'a Input) -> Self {
        Self {
            paths: store::transfer_agreement::Paths::new(&input.promises),
            people: People::new(input),
        }
    }

    fn for_promise(&self, promise: &'a Promise) -> Result<Roles<'a>, &'static str> {
        nucleus::performance::add("protein-transfer-role-results", 1);
        if promise.state == "open" {
            return Err("open_promise_template");
        }
        let actor = promise
            .person_uid
            .as_deref()
            .ok_or("promise_person_unassigned")?;
        if let Some(exchange) = &promise.exchange {
            if actor != exchange.owner(promise.delta) {
                return Err("exchange_direction_invalid");
            }
            let other = if actor == exchange.giver {
                &exchange.receiver
            } else {
                &exchange.giver
            };
            let people = &self.people;
            if !people.eligible(actor, Some(other)) || !people.accepted().contains(other.as_str()) {
                return Err("exchange_counterparty_unavailable");
            }
            let mut matches = self.paths.matching(promise);
            let first = matches.next();
            if matches.next().is_some() {
                return Err("exchange_allocation_ambiguous");
            }
            if first.is_some_and(|other| !matches!(other.state.as_str(), "agreed" | "active")) {
                return Err("exchange_counterparty_not_agreed");
            }
            return Ok(Roles {
                opposite: first.map(|other| other.uid.as_str()),
                giver: &exchange.giver,
                receiver: &exchange.receiver,
            });
        }

        let people = &self.people;
        let mut exact = self.paths.matching(promise).filter(|other| {
            matches!(other.state.as_str(), "agreed" | "active")
                && people.eligible(actor, other.person_uid.as_deref())
        });
        let first = exact.next();
        if exact.next().is_some() {
            return Err("occurrence_counterparty_ambiguous");
        }
        let (opposite, counterparty) = if let Some(other) = first {
            (
                Some(other.uid.as_str()),
                other
                    .person_uid
                    .as_deref()
                    .ok_or("occurrence_counterparty_missing")?,
            )
        } else {
            let mut others = people
                .fallback()
                .iter()
                .copied()
                .filter(|person| *person != actor);
            let other = others.next().ok_or("occurrence_counterparty_ambiguous")?;
            if others.next().is_some() {
                return Err("occurrence_counterparty_ambiguous");
            }
            (None, other)
        };
        if promise.delta < 0.0 {
            Ok(Roles {
                opposite,
                giver: actor,
                receiver: counterparty,
            })
        } else if promise.delta > 0.0 {
            Ok(Roles {
                opposite,
                giver: counterparty,
                receiver: actor,
            })
        } else {
            Err("occurrence_quantity_zero")
        }
    }
}

pub(super) fn one(
    input: &Input,
    promise_uid: &str,
) -> Result<(Option<String>, String, String), &'static str> {
    let _roles = nucleus::performance::Span::new("protein-transfer-roles");
    let promise = input
        .promises
        .iter()
        .find(|promise| promise.uid == promise_uid)
        .ok_or("promise_not_in_current_revision")?;
    let roles = Prepared::new(input).for_promise(promise)?;
    Ok((
        roles.opposite.map(str::to_owned),
        roles.giver.to_owned(),
        roles.receiver.to_owned(),
    ))
}

pub(super) fn validity(input: &Input) -> std::collections::HashMap<&str, Result<(), &'static str>> {
    let _roles = nucleus::performance::Span::new("protein-transfer-roles");
    let prepared = Prepared::new(input);
    let mut results = std::collections::HashMap::with_capacity(input.promises.len());
    for promise in &input.promises {
        results
            .entry(promise.uid.as_str())
            .or_insert_with(|| prepared.for_promise(promise).map(|_| ()));
    }
    results
}
