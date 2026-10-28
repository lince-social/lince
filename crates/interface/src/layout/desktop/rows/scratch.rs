use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::layout::desktop) struct ScratchHome {
    slot: Home,
    floating: bool,
}

impl ScratchHome {
    pub(in crate::layout::desktop) fn geometry(&self) -> Geometry {
        self.slot.free
    }

    pub(in crate::layout::desktop) fn valid(&self, id: PlacementId) -> bool {
        self.slot.item.id == id
            && self.slot.row != 0
            && self.slot.row_index <= MAX_ITEMS
            && self.slot.index <= MAX_ITEMS
            && self.slot.before != Some(id)
            && self.slot.after != Some(id)
            && [self.slot.before, self.slot.after].into_iter().flatten().all(|id| id.0 != [0; 16])
            && self.slot.item.width.valid()
            && self.slot.height.valid()
            && self.slot.free.rectangle().is_some()
            && self.slot.free_width.is_none_or(FreeWidth::valid)
            && self.slot.free_sizing.into_iter().all(SizeIntent::valid)
            && self.slot.item.floating.is_none_or(|geometry| geometry.rectangle().is_some())
            && self.slot.item.floating_width.is_none_or(FreeWidth::valid)
            && self.slot.item.floating_sizing.is_none_or(|sizing| sizing.into_iter().all(SizeIntent::valid))
    }

    pub(in crate::layout::desktop) fn width(&self) -> Width {
        self.slot.item.width
    }

    pub(in crate::layout::desktop) fn restore(&self, rows: &mut Rows, item: &mut Item, original_slot: bool) {
        let mut home = self.slot.clone();
        if !original_slot {
            if let Some(current) = rows.homes.iter().find(|home| home.item.id == item.id) {
                home.row = current.row;
                home.row_index = current.row_index;
                home.index = current.index;
                home.before = current.before;
                home.after = current.after;
            } else if let Some((row, index)) = rows.location(item.id) {
                let current = &rows.rows[row];
                home.row = current.id;
                home.row_index = row;
                home.index = index;
                home.before = index.checked_sub(1).map(|index| current.items[index].id);
                home.after = current.items.get(index + 1).map(|item| item.id);
            } else {
                home.row = rows.fresh_id();
                home.row_index = rows.rows.len();
                home.index = 0;
                home.before = None;
                home.after = None;
            }
        }
        for row in &mut rows.rows { row.items.retain(|member| member.id != item.id); }
        rows.rows.retain(|row| !row.items.is_empty());
        rows.homes.retain(|home| home.item.id != item.id);
        item.free = self.slot.free;
        item.free_width = self.slot.free_width;
        item.floating = self.floating;
        SizeIntent::restore(self.slot.free_sizing, &mut item.rules);
        if self.floating { rows.homes.push(home); } else { rows.insert_home(home); }
        if rows.last_focus == Some(item.id) && self.floating { rows.last_focus = None; }
    }
}

impl Rows {
    pub(in crate::layout::desktop) fn scratch_home(&mut self, item: &Item) -> Result<ScratchHome, LayoutError> {
        let mut slot = if !item.floating {
            self.home(item)?
        } else if let Some(home) = self.homes.iter().find(|home| home.item.id == item.id) {
            home.clone()
        } else {
            Home {
                item: Member { id: item.id, width: Width::Half, floating: None, floating_width: None, floating_sizing: None },
                row: self.fresh_id(), row_index: self.rows.len(), index: 0, before: None, after: None,
                height: Width::Half, free: item.free, free_width: item.free_width,
                free_sizing: SizeIntent::capture(item.rules),
            }
        };
        slot.free = item.free;
        slot.free_width = item.free_width;
        slot.free_sizing = SizeIntent::capture(item.rules);
        Ok(ScratchHome { slot, floating: item.floating })
    }
}
