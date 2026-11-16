use super::*;

pub(in crate::layout::desktop) struct Membership {
    pub(in crate::layout::desktop) grouped: BTreeSet<PlacementId>,
    pub(in crate::layout::desktop) arranged: BTreeSet<PlacementId>,
}

impl Rows {
    /// The workspace already established unique, valid item identities. Reuse
    /// that index and its non-floating set for this invocation's row checks.
    pub(in crate::layout::desktop) fn validated(
        &self,
        all: &BTreeMap<PlacementId, &Item>,
        tiled: &BTreeSet<PlacementId>,
    ) -> Option<Membership> {
        #[cfg(feature = "native-presentation")]
        let _validation = engine::performance::Span::new("desktop-row-validation");
        if self.next_id == 0
            || self.rows.len() > all.len()
            || self.homes.len() > all.len()
            || !self
                .scroll
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
        {
            return None;
        }
        let member_valid = |member: &Member| {
            member.width.valid()
                && member.floating_width.is_none_or(FreeWidth::valid)
                && member
                    .floating_sizing
                    .is_none_or(|sizing| sizing.into_iter().all(SizeIntent::valid))
                && member
                    .floating
                    .is_none_or(|geometry| geometry.rectangle().is_some())
        };
        let mut arranged = BTreeSet::new();
        let mut row_ids = BTreeSet::new();
        for row in &self.rows {
            if row.id == 0
                || !row_ids.insert(row.id)
                || !row.height.valid()
                || row.items.is_empty()
                || !row
                    .items
                    .iter()
                    .all(|item| member_valid(item) && arranged.insert(item.id))
            {
                return None;
            }
        }
        let mut homes = BTreeSet::new();
        for home in &self.homes {
            if !homes.insert(home.item.id)
                || home.row == 0
                || home.index > MAX_ITEMS
                || home.row_index > MAX_ITEMS
                || !home.height.valid()
                || !member_valid(&home.item)
                || home.free.rectangle().is_none()
                || !home.free_width.is_none_or(FreeWidth::valid)
                || !home.free_sizing.into_iter().all(SizeIntent::valid)
                || ![home.before, home.after]
                    .into_iter()
                    .flatten()
                    .all(|id| id != home.item.id && id.0 != [0; 16])
            {
                return None;
            }
        }
        let mut grouped = BTreeSet::new();
        for group in &self.compact {
            if group.members.len() < 2
                || group.members.len() > MAX_ITEMS
                || !group.members.contains(&group.active)
                || !arranged.contains(&group.active)
                || !group.members.iter().all(|id| {
                    all.get(id).is_some_and(|item| !item.floating)
                        && grouped.insert(*id)
                        && homes.contains(id)
                        && (*id == group.active || !arranged.contains(id))
                })
            {
                return None;
            }
        }
        // Each group has exactly its active member arranged. Everything else
        // non-floating must be arranged; there are no extra row identities.
        // This is the original expected-set subtraction without a second copy.
        if !arranged.is_subset(tiled)
            || !tiled
                .iter()
                .all(|id| arranged.contains(id) || grouped.contains(id))
            || !homes
                .iter()
                .all(|id| all.get(id).is_some_and(|item| item.floating) || grouped.contains(id))
            || !self.last_focus.is_none_or(|id| arranged.contains(&id))
            || !self.free_view.is_none_or(|(center, zoom)| {
                center
                    .into_iter()
                    .all(|value| value.is_finite() && value.abs() <= 1_000_000_000.0)
                    && zoom.is_finite()
                    && zoom > 0.0
            })
        {
            return None;
        }
        Some(Membership { grouped, arranged })
    }
}
