include!(concat!(env!("OUT_DIR"), "/bundle.rs"));

use std::collections::{BTreeMap, HashMap};

pub const VOCABULARY: [&str; 11] = [
    "idea",
    "chapter",
    "position",
    "reference",
    "instinct",
    "task",
    "part-of",
    "done",
    "backlog",
    "todo",
    "wip",
];

#[derive(Debug, Clone)]
pub struct Projection {
    pub uid: String,
    pub assertions: Vec<Line>,
    pub quantity: Option<(String, Option<String>)>,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub predicate: String,
    pub identity: bool,
    pub object: Option<Link>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub title: String,
    pub uid: String,
}

#[derive(Debug, Clone)]
pub struct BundledRecord {
    pub slug: Option<String>,
    pub head: String,
    pub projection: Projection,
    pub body: String,
}

impl BundledRecord {
    pub fn identity(&self) -> &str {
        self.projection
            .assertions
            .iter()
            .find(|line| line.identity)
            .map(|line| line.predicate.as_str())
            .unwrap_or("")
    }

    pub fn quantity(&self) -> Option<String> {
        self.projection
            .quantity
            .as_ref()
            .map(|(quantity, _)| quantity.clone())
    }

    fn parent_line(&self) -> Option<&Line> {
        self.projection
            .assertions
            .iter()
            .find(|line| line.predicate == "part-of" && line.object.is_some())
    }

    pub fn is_root(&self) -> bool {
        self.parent_line().is_none()
    }

    pub fn position(&self) -> f64 {
        self.parent_line()
            .or_else(|| {
                self.projection
                    .assertions
                    .iter()
                    .find(|line| line.predicate == "position")
            })
            .and_then(|line| line.quantity.as_deref())
            .and_then(|value| value.parse().ok())
            .unwrap_or(f64::MAX)
    }

    pub fn parent_uid(&self) -> Option<&str> {
        self.parent_line()?
            .object
            .as_ref()
            .map(|link| link.uid.as_str())
    }

    pub fn path(&self, all: &[BundledRecord]) -> Vec<f64> {
        let mut output = vec![self.position()];
        let mut seen = vec![self.projection.uid.as_str()];
        let mut current = self;
        while let Some(parent) = current.parent_uid() {
            if seen.contains(&parent) {
                break;
            }
            let Some(owner) = all.iter().find(|record| record.projection.uid == parent) else {
                break;
            };
            output.push(owner.position());
            seen.push(parent);
            current = owner;
        }
        output.reverse();
        output
    }

    pub fn is_entry(&self) -> bool {
        self.is_root() || self.identity() == "chapter"
    }

    pub fn entry_uid<'a>(&'a self, all: &'a [BundledRecord]) -> &'a str {
        let mut current = self;
        let mut seen = vec![current.projection.uid.as_str()];
        while !current.is_entry() {
            let Some(parent) = current.parent_uid() else {
                break;
            };
            if seen.contains(&parent) {
                break;
            }
            let Some(owner) = all.iter().find(|record| record.projection.uid == parent) else {
                break;
            };
            seen.push(parent);
            current = owner;
        }
        current.projection.uid.as_str()
    }
}

pub fn records() -> Vec<BundledRecord> {
    let mut projected = Vec::new();
    for (name, source) in BUNDLE {
        projected.extend(project_source(name, source));
    }
    projected.retain(|record| {
        record
            .assertions
            .iter()
            .any(|assertion| !assertion.identity && assertion.predicate == "instinct")
    });

    let identities: BTreeMap<String, String> = projected
        .iter()
        .filter_map(|record| {
            record
                .slug
                .as_ref()
                .map(|slug| (slug.clone(), record.uid.clone()))
        })
        .collect();
    let titles: HashMap<String, String> = projected
        .iter()
        .map(|record| (record.uid.clone(), record.head.clone()))
        .collect();

    let mut output: Vec<BundledRecord> = projected
        .into_iter()
        .map(|record| {
            let assertions = record
                .assertions
                .into_iter()
                .map(|assertion| {
                    let object = assertion.object_slug.as_ref().map(|slug| {
                        let uid = identities
                            .get(slug)
                            .unwrap_or_else(|| panic!("@{slug} does not resolve in anicca/"))
                            .clone();
                        Link {
                            title: titles.get(&uid).cloned().unwrap_or_default(),
                            uid,
                        }
                    });
                    Line {
                        predicate: assertion.predicate,
                        identity: assertion.identity,
                        object,
                        quantity: assertion.quantity,
                        unit: assertion.unit,
                    }
                })
                .collect();
            BundledRecord {
                slug: record.slug,
                head: record.head,
                projection: Projection {
                    uid: record.uid,
                    assertions,
                    quantity: record.quantity,
                },
                body: record.body,
            }
        })
        .collect();

    let snapshot = output.clone();
    let paths: HashMap<String, Vec<f64>> = snapshot
        .iter()
        .map(|record| (record.projection.uid.clone(), record.path(&snapshot)))
        .collect();
    output.sort_by(|left, right| {
        let empty = Vec::new();
        let left_path = paths.get(&left.projection.uid).unwrap_or(&empty);
        let right_path = paths.get(&right.projection.uid).unwrap_or(&empty);
        left_path
            .iter()
            .zip(right_path.iter())
            .find_map(|(left, right)| match left.total_cmp(right) {
                std::cmp::Ordering::Equal => None,
                ordering => Some(ordering),
            })
            .unwrap_or_else(|| left_path.len().cmp(&right_path.len()))
            .then(left.head.cmp(&right.head))
    });
    output
}

fn project_source(name: &str, source: &str) -> Vec<anicca::ProjectedRecord> {
    let (identified, _) = anicca::ensure_uids(source)
        .unwrap_or_else(|error| panic!("anicca/{name} cannot receive identities: {error}"));
    let document = anicca::parse(&identified)
        .unwrap_or_else(|error| panic!("anicca/{name} is malformed: {error}"));
    anicca::project(&document)
        .unwrap_or_else(|error| panic!("anicca/{name} cannot be projected: {error}"))
        .records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_records_receive_missing_identities_before_projection() {
        let records = project_source(
            "Example.lingua",
            "Example (@example: 1, #instinct) {\nText.\n}\n",
        );

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].slug.as_deref(), Some("example"));
        assert!(records[0].uid.starts_with("r_"));
    }

    #[test]
    fn bundle_is_the_valid_root_anicca_tree() {
        let records = records();
        assert!(!records.is_empty());
        let roots: Vec<&str> = records
            .iter()
            .filter(|record| record.is_root())
            .map(|record| record.head.as_str())
            .collect();
        assert!(!roots.is_empty());
        assert!(roots.contains(&records[0].head.as_str()));
    }

    #[test]
    fn every_parent_precedes_its_children() {
        let records = records();
        for (index, record) in records.iter().enumerate() {
            let Some(parent) = record.parent_uid() else {
                continue;
            };
            let owner = records
                .iter()
                .position(|candidate| candidate.projection.uid == parent)
                .unwrap_or_else(|| panic!("{} has no bundled parent", record.head));
            assert!(owner < index, "{} precedes its parent", record.head);
        }
    }
}
