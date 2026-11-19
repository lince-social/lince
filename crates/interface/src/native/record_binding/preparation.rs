use super::*;

#[derive(Default)]
pub(super) struct Views {
    ordered: Vec<(Entity, TextBinding)>,
    groups: Documents<Vec<(Entity, TextBinding)>>,
}

impl Views {
    pub(super) fn clear(&mut self) {
        self.ordered.clear();
        self.ordered.shrink_to_fit();
        self.groups.clear();
        self.groups.shrink_to_fit();
    }

    pub(super) fn collect(
        &mut self,
        world: &World,
        query: &mut QueryState<(Entity, &'static TextBinding)>,
        focus: Option<Entity>,
    ) {
        let _preparation = performance::Span::new(Stage::RecordBindingPreparation);
        self.ordered.clear();
        self.ordered.extend(
            query
                .iter(world)
                .filter(|(entity, _)| !crate::native::laboratory::blocked(world, *entity))
                .map(|(entity, binding)| (entity, binding.clone())),
        );
        if let Some(index) =
            focus.and_then(|focus| self.ordered.iter().position(|(entity, _)| *entity == focus))
        {
            self.ordered[..=index].rotate_right(1);
        }
        performance::add(
            Count::RecordBindingSharedSnapshots,
            self.ordered.len() as u64,
        );
    }

    pub(super) fn ordered(&self) -> &[(Entity, TextBinding)] {
        &self.ordered
    }

    pub(super) fn group(&mut self) {
        let _grouping = performance::Span::new(Stage::RecordBindingGrouping);
        for (entity, binding) in self.ordered.drain(..) {
            self.groups
                .entry_ref(&key_ref(&binding.record))
                .or_insert_with(|| {
                    performance::count(Count::RecordBindingGroupsCreated);
                    Vec::new()
                })
                .push((entity, binding));
        }
        self.groups.retain(|_, members| !members.is_empty());
        if self.groups.is_empty() {
            self.ordered.shrink_to_fit();
            self.groups.shrink_to_fit();
        }
        performance::add(Count::RecordBindingPreparedGroups, self.groups.len() as u64);
    }

    pub(super) fn members(&self, key: &Key) -> &[(Entity, TextBinding)] {
        self.groups.get(key).map_or(&[], Vec::as_slice)
    }

    pub(super) fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    pub(super) fn release(&mut self) {
        for members in self.groups.values_mut() {
            members.clear();
        }
    }
}
