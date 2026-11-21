// non-lean-yet
// Scope: enum Subject, struct Key, type Groups, struct Paths.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 135, 194.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 80, 186.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 35.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 35, 41,
//   85.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   93, 96, 100, 109, 110, 193.

use std::{cell::OnceCell, collections::HashMap, iter::Peekable, slice};

use nucleus::transfer::{TransferDependencyScope, TransferRevisionDependency as Dependency};

use super::agreement_path_matches;
use crate::transfers::AgreementPromiseReadinessRow as Promise;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Subject<'a> {
    Exchange(&'a str),
    Concept(&'a str),
    Record(&'a str),
    RecordWithoutConcept(&'a str),
}

#[derive(PartialEq, Eq, Hash)]
struct Key<'a> {
    subject: Subject<'a>,
    unit: Option<&'a str>,
    start: Option<&'a str>,
    end: Option<&'a str>,
}

impl<'a> Key<'a> {
    fn new(promise: &'a Promise, subject: Subject<'a>) -> Self {
        Self {
            subject,
            unit: promise.unit_uid.as_deref(),
            start: promise.window_start.as_deref(),
            end: promise.window_end.as_deref(),
        }
    }
}

type Groups<'a> = HashMap<Key<'a>, Vec<usize>>;

pub struct Paths<'a> {
    promises: &'a [Promise],
    groups: OnceCell<Groups<'a>>,
}

impl<'a> Paths<'a> {
    pub fn new(promises: &'a [Promise]) -> Self {
        Self {
            promises,
            groups: OnceCell::new(),
        }
    }

    fn prepare(&self) -> Groups<'a> {
        let _index = nucleus::performance::Span::new("transfer-agreement-path-index");
        let mut groups: Groups<'a> = HashMap::new();
        for (index, promise) in self.promises.iter().enumerate() {
            if let Some(exchange) = &promise.exchange {
                groups
                    .entry(Key::new(promise, Subject::Exchange(&exchange.uid)))
                    .or_default()
                    .push(index);
            } else {
                if let Some(record) = promise.record_uid.as_deref() {
                    groups
                        .entry(Key::new(promise, Subject::Record(record)))
                        .or_default()
                        .push(index);
                    if promise.concept_uid.is_none() {
                        groups
                            .entry(Key::new(promise, Subject::RecordWithoutConcept(record)))
                            .or_default()
                            .push(index);
                    }
                }
                if let Some(concept) = promise.concept_uid.as_deref() {
                    groups
                        .entry(Key::new(promise, Subject::Concept(concept)))
                        .or_default()
                        .push(index);
                }
            }
        }
        nucleus::performance::add(
            "transfer-agreement-path-indexed-promises",
            self.promises.len() as u64,
        );
        groups
    }

    pub fn matching<'p>(&'p self, promise: &'a Promise) -> impl Iterator<Item = &'a Promise> + 'p {
        let groups = (promise.exchange.is_some()
            || promise.concept_uid.is_some()
            || promise.record_uid.is_some())
        .then(|| self.groups.get_or_init(|| self.prepare()));
        let group = |subject| {
            groups
                .and_then(|groups| groups.get(&Key::new(promise, subject)))
                .map_or(&[][..], Vec::as_slice)
        };
        let (left, right) = if let Some(exchange) = &promise.exchange {
            (group(Subject::Exchange(&exchange.uid)), &[][..])
        } else if let Some(concept) = promise.concept_uid.as_deref() {
            (
                group(Subject::Concept(concept)),
                promise.record_uid.as_deref().map_or(&[][..], |record| {
                    group(Subject::RecordWithoutConcept(record))
                }),
            )
        } else {
            (
                promise
                    .record_uid
                    .as_deref()
                    .map_or(&[][..], |record| group(Subject::Record(record))),
                &[][..],
            )
        };
        nucleus::performance::add("transfer-agreement-path-lookups", 1);
        Matches {
            promises: self.promises,
            promise,
            positions: Positions::new(left, right),
            examined: 0,
        }
    }
}

struct Matches<'p, 'a> {
    promises: &'a [Promise],
    promise: &'a Promise,
    positions: Positions<'p>,
    examined: u64,
}

impl<'a> Iterator for Matches<'_, 'a> {
    type Item = &'a Promise;

    fn next(&mut self) -> Option<Self::Item> {
        for index in self.positions.by_ref() {
            let candidate = &self.promises[index];
            self.examined += 1;
            if agreement_path_matches(self.promise, candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

impl Drop for Matches<'_, '_> {
    fn drop(&mut self) {
        nucleus::performance::add("transfer-agreement-path-candidates", self.examined);
    }
}

#[derive(Default)]
struct DependencyGroups<'a> {
    transfer: Vec<usize>,
    promises: HashMap<&'a str, Vec<usize>>,
}

pub(super) struct Dependencies<'a> {
    source: &'a [Dependency],
    groups: OnceCell<DependencyGroups<'a>>,
}

impl<'a> Dependencies<'a> {
    pub(super) fn new(source: &'a [Dependency]) -> Self {
        Self {
            source,
            groups: OnceCell::new(),
        }
    }

    pub(super) fn matching<'p>(
        &'p self,
        promise_uid: &'p str,
    ) -> impl Iterator<Item = &'a Dependency> + 'p {
        let groups = self.groups.get_or_init(|| {
            let _index = nucleus::performance::Span::new("transfer-agreement-dependency-index");
            let mut groups = DependencyGroups::default();
            for (index, dependency) in self.source.iter().enumerate() {
                if dependency.scope == TransferDependencyScope::Transfer {
                    groups.transfer.push(index);
                } else if let Some(promise) = dependency.promise_uid.as_deref() {
                    groups.promises.entry(promise).or_default().push(index);
                }
            }
            nucleus::performance::add(
                "transfer-agreement-dependency-source-rows",
                self.source.len() as u64,
            );
            groups
        });
        let local = groups
            .promises
            .get(promise_uid)
            .map_or(&[][..], Vec::as_slice);
        Positions::new(&groups.transfer, local).map(move |index| &self.source[index])
    }
}

struct Positions<'a> {
    left: Peekable<slice::Iter<'a, usize>>,
    right: Peekable<slice::Iter<'a, usize>>,
}

impl<'a> Positions<'a> {
    fn new(left: &'a [usize], right: &'a [usize]) -> Self {
        Self {
            left: left.iter().peekable(),
            right: right.iter().peekable(),
        }
    }
}

impl Iterator for Positions<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let index = match (
            self.left.peek().map(|&&i| i),
            self.right.peek().map(|&&i| i),
        ) {
            (Some(left), Some(right)) if left == right => {
                self.right.next();
                self.left.next()
            }
            (Some(left), Some(right)) if left < right => self.left.next(),
            (Some(_), Some(_)) | (None, Some(_)) => self.right.next(),
            (Some(_), None) => self.left.next(),
            (None, None) => None,
        };
        index.copied()
    }
}
