use gpui::{BorrowAppContext, Global};
use std::collections::HashMap;

#[derive(Default)]
pub struct CollectionViewColumnWidthsGlobal {
    pub widths_by_collection_view: HashMap<u32, HashMap<String, f32>>,
}

impl Global for CollectionViewColumnWidthsGlobal {}

pub fn set_collection_view_column_widths<C>(
    cx: &mut C,
    widths_by_collection_view: HashMap<u32, HashMap<String, f32>>,
) where
    C: BorrowAppContext,
{
    cx.update_default_global::<CollectionViewColumnWidthsGlobal, _>(|global, _| {
        global.widths_by_collection_view = widths_by_collection_view;
    });
}

pub fn get_collection_view_column_widths<C>(
    cx: &mut C,
    collection_view_id: u32,
) -> HashMap<String, f32>
where
    C: BorrowAppContext,
{
    cx.update_default_global::<CollectionViewColumnWidthsGlobal, _>(|global, _| {
        global
            .widths_by_collection_view
            .get(&collection_view_id)
            .cloned()
            .unwrap_or_default()
    })
}

pub fn update_collection_view_column_widths<C>(
    cx: &mut C,
    collection_view_id: u32,
    widths: HashMap<String, f32>,
) where
    C: BorrowAppContext,
{
    cx.update_default_global::<CollectionViewColumnWidthsGlobal, _>(|global, _| {
        global
            .widths_by_collection_view
            .insert(collection_view_id, widths);
    });
}
