use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferKind {
    Item,
    Row,
}

impl Workspace {
    pub fn transfer_to(
        &mut self,
        destination: &mut Self,
        id: PlacementId,
        kind: TransferKind,
    ) -> Result<Vec<PlacementId>, LayoutError> {
        if !self.valid() || !destination.valid() {
            return Err(LayoutError::InvalidState);
        }
        let item = self
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or(LayoutError::MissingItem)?;
        if kind == TransferKind::Row && (self.mode != WorkspaceLayout::Tiled || item.floating) {
            return Err(LayoutError::InvalidState);
        }
        let parcel = self.rows.snapshot_transfer(id, kind)?;
        let ids = parcel.ids.clone();
        if destination.items.len() + ids.len() > MAX_ITEMS {
            return Err(LayoutError::TooManyItems);
        }
        if destination.items.iter().any(|item| ids.contains(&item.id)) {
            return Err(LayoutError::DuplicateItem);
        }
        let state: Vec<_> = ids
            .iter()
            .map(|id| {
                (
                    *id,
                    self.maximized(*id),
                    self.minimized.contains(id),
                    self.minimized_fullscreen.contains(id),
                    self.windowed_fullscreen(*id),
                    self.scratch.get(id).cloned(),
                    self.column(*id)
                        .map(|index| self.columns[index].width)
                        .unwrap_or(Width::Half),
                )
            })
            .collect();
        let fullscreen = self.fullscreen.filter(|id| ids.contains(id));
        let desired = fullscreen
            .or(Some(id).filter(|id| !self.hidden(*id)))
            .or(self
                .focused
                .filter(|id| ids.contains(id) && !self.hidden(*id)))
            .or_else(|| ids.iter().copied().find(|id| !self.hidden(*id)));
        let near = destination
            .focused
            .filter(|id| destination.rows.contains(*id))
            .or(destination.rows.last_focus);
        let previous_focus = destination.focused;
        let mut source = self.clone();
        let mut target = destination.clone();
        for (id, maximized, minimized, minimized_fullscreen, windowed_fullscreen, scratch, width) in state {
            let item = source.remove(id).ok_or(LayoutError::MissingItem)?;
            target.insert(item)?;
            if maximized {
                target.maximized.insert(id);
            }
            if minimized {
                target.minimized.insert(id);
            }
            if minimized_fullscreen {
                target.minimized_fullscreen.insert(id);
            }
            if windowed_fullscreen { target.windowed_fullscreen.insert(id); }
            if let Some(scratch) = scratch { target.scratch.insert(id, scratch); }
            if let Some(column) = target.column(id) {
                target.columns[column].width = width;
                target.columns[column].maximized = maximized;
            }
        }
        target
            .rows
            .receive_transfer(parcel, near, &target.minimized);
        target.focused = desired.or(previous_focus);
        if let Some(group) = target.focused.and_then(|id| target.compact_group(id)) {
            target.focused = Some(group.active);
        }
        if let Some(focus) = desired {
            target.fullscreen = fullscreen;
            if target.rows.contains(focus) {
                target.rows.last_focus = Some(focus);
            }
        }
        if !source.valid() || !target.valid() {
            return Err(LayoutError::InvalidState);
        }
        *self = source;
        *destination = target;
        Ok(ids)
    }
}
