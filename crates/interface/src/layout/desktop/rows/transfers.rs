use super::*;

pub(in crate::layout::desktop) struct Parcel {
    pub(in crate::layout::desktop) ids: Vec<PlacementId>,
    members: Vec<Member>,
    homes: Vec<Home>,
    compact: Vec<CompactGroup>,
    height: Width,
    row: bool,
}

impl Rows {
    pub(in crate::layout::desktop) fn snapshot_transfer(
        &self,
        id: PlacementId,
        kind: TransferKind,
    ) -> Result<Parcel, LayoutError> {
        if kind == TransferKind::Row {
            let (index, _) = self.location(id).ok_or(LayoutError::MissingItem)?;
            let row = &self.rows[index];
            let ids: Vec<_> = row.items.iter().flat_map(|item| self.compact.iter().find(|group| group.active == item.id)
                .map_or_else(|| vec![item.id], |group| group.members.clone())).collect();
            return Ok(Parcel {
                homes: self.homes.iter().filter(|home| ids.contains(&home.item.id)).cloned().collect(),
                compact: self.compact.iter().filter(|group| ids.contains(&group.active)).cloned().collect(),
                ids,
                members: row.items.clone(),
                height: row.height,
                row: true,
            });
        }
        let group = self.compact.iter().find(|group| group.members.contains(&id));
        let ids = group.map_or_else(|| vec![id], |group| group.members.clone());
        let active = group.map_or(id, |group| group.active);
        let member = self
            .location(active)
            .map(|(row, index)| (self.rows[row].items[index].clone(), self.rows[row].height));
        let homes: Vec<_> = self
            .homes
            .iter()
            .filter(|home| ids.contains(&home.item.id))
            .cloned()
            .collect();
        let height = member
            .as_ref()
            .map(|(_, height)| *height)
            .or_else(|| homes.first().map(|home| home.height))
            .unwrap_or(Width::Full);
        Ok(Parcel {
            ids,
            compact: group.into_iter().cloned().collect(),
            members: member.into_iter().map(|(member, _)| member).collect(),
            homes,
            height,
            row: false,
        })
    }

    pub(in crate::layout::desktop) fn receive_transfer(
        &mut self,
        parcel: Parcel,
        near: Option<PlacementId>,
        minimized: &BTreeSet<PlacementId>,
    ) {
        for id in &parcel.ids {
            self.forget(*id, minimized);
        }
        if parcel.row {
            let index = near
                .and_then(|id| self.location(id))
                .map_or(self.rows.len(), |(row, _)| row + 1);
            let id = self.fresh_id();
            self.rows.insert(
                index,
                Row {
                    id,
                    height: parcel.height,
                    items: parcel.members,
                },
            );
        } else if let Some(member) = parcel.members.into_iter().next() {
            if let Some((row, index)) = near.and_then(|id| self.location(id)) {
                self.rows[row].items.insert(index + 1, member);
            } else if let Some(row) = self.rows.last_mut() {
                row.items.push(member);
            } else {
                let id = self.fresh_id();
                self.rows.push(Row {
                    id,
                    height: parcel.height,
                    items: vec![member],
                });
            }
        }
        for mut home in parcel.homes {
            if let Some(group) = parcel.compact.iter().find(|group| group.members.contains(&home.item.id))
                && let Some((row_index, slot)) = self.location(group.active)
            {
                let row = &self.rows[row_index];
                let member = group.members.iter().position(|id| *id == home.item.id).unwrap();
                home.row = row.id;
                home.row_index = row_index;
                home.index = slot + member;
                home.before = member.checked_sub(1).map(|index| group.members[index])
                    .or_else(|| slot.checked_sub(1).and_then(|index| row.items.get(index)).map(|item| item.id));
                home.after = row.items.get(slot + 1).map(|item| item.id);
                home.height = row.height;
                self.homes.push(home);
                continue;
            }
            let target = near
                .and_then(|id| self.location(id))
                .map(|(row, index)| (row, index + 1))
                .or_else(|| {
                    self.rows
                        .last()
                        .map(|row| (self.rows.len() - 1, row.items.len()))
                });
            if let Some((row_index, index)) = target {
                let row = &self.rows[row_index];
                home.row = row.id;
                home.row_index = row_index;
                home.index = index;
                home.before = index
                    .checked_sub(1)
                    .and_then(|index| row.items.get(index))
                    .map(|item| item.id);
                home.after = row.items.get(index).map(|item| item.id);
                home.height = row.height;
            } else {
                home.row = self.fresh_id();
                home.row_index = self.rows.len();
                home.index = 0;
                home.before = None;
                home.after = None;
            }
            self.homes.push(home);
        }
        self.compact.extend(parcel.compact);
        if let Some(id) = parcel.ids.into_iter().find(|id| self.contains(*id)) {
            self.last_focus = Some(id);
        }
    }
}

#[cfg(test)]
mod tests;
