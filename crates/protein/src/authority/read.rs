use super::*;

pub struct ReadGraph<'snapshot, 'limits> {
    graph: Graph<'snapshot>,
    baseline: Budget<'limits>,
}

impl<'snapshot, 'limits> ReadGraph<'snapshot, 'limits> {
    pub fn new(
        snapshot: &'snapshot GraphSnapshot,
        limits: &'limits Limits,
    ) -> Result<Self, AuthorityError> {
        let mut baseline = Budget::new(limits);
        let graph = Graph::prepare(snapshot, &mut baseline)?;
        nucleus::performance::add("authority-read-graphs-prepared", 1);
        Ok(Self { graph, baseline })
    }

    pub fn readable_records(
        &self,
        policy: &RolePolicy,
        ceiling: &VisibilityCeiling,
    ) -> Result<BTreeSet<String>, AuthorityError> {
        let _read = nucleus::performance::Span::new("authority-prepared-read");
        nucleus::performance::add("authority-read-policy-evaluations", 1);
        let mut budget = self.baseline.clone();
        let selectors = {
            let _preparation = nucleus::performance::Span::new("authority-read-policy-preparation");
            Selectors::prepare(policy, &self.graph, ceiling, &mut budget)?
        };
        collect(&self.graph, &selectors.read, ceiling, &mut budget)
    }
}

pub(super) fn collect(
    graph: &Graph<'_>,
    read: &Selector<'_>,
    ceiling: &VisibilityCeiling,
    budget: &mut Budget<'_>,
) -> Result<BTreeSet<String>, AuthorityError> {
    let mut readable = BTreeSet::new();
    for record in graph.records.values() {
        budget.step()?;
        if !record.deleted
            && ceiling.records.contains(&record.uid)
            && read.matches(record, budget)?
        {
            readable.insert(record.uid.clone());
        }
    }
    Ok(readable)
}
