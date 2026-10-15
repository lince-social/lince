use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    text::{FontAtlasSet, TextLayoutInfo},
};
use std::collections::HashSet;

pub const CACHE_BYTES: u64 = 64 * 1024 * 1024;

pub fn trim(
    mut frame: Local<u32>,
    atlases: Option<ResMut<FontAtlasSet>>,
    images: Option<Res<Assets<Image>>>,
    layouts: Query<&TextLayoutInfo, Allow<Disabled>>,
) {
    *frame = frame.wrapping_add(1);
    if !frame.is_multiple_of(120) {
        return;
    }
    let (Some(mut atlases), Some(images)) = (atlases, images) else {
        return;
    };
    if atlases.total_bytes(&images) <= CACHE_BYTES {
        return;
    }
    let used: HashSet<_> = layouts
        .iter()
        .flat_map(|layout| layout.glyphs.iter().map(|glyph| glyph.atlas_info.texture))
        .collect();
    retain_used(&mut atlases, &used);
}

fn retain_used(atlases: &mut FontAtlasSet, used: &HashSet<AssetId<Image>>) {
    atlases.retain(|_, pages| pages.iter().any(|page| used.contains(&page.texture.id())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::text::{FontAtlas, FontAtlasKey};

    #[test]
    fn live_glyphs_keep_page_indices_while_unused_font_sizes_are_released() {
        let mut images = Assets::<Image>::default();
        let mut atlases = FontAtlasSet::default();
        let key = FontAtlasKey {
            id: 1,
            index: 0,
            font_size_bits: 16.0_f32.to_bits(),
            variations_hash: 0,
            hinting: default(),
            font_smoothing: default(),
        };
        let first = FontAtlas::new(&mut images, UVec2::splat(16), default());
        let second = FontAtlas::new(&mut images, UVec2::splat(16), default());
        let used = HashSet::from([second.texture.id()]);
        let order = [first.texture.id(), second.texture.id()];
        atlases.insert(key, vec![first, second]);
        atlases.insert(
            FontAtlasKey {
                font_size_bits: 24.0_f32.to_bits(),
                ..key
            },
            vec![FontAtlas::new(&mut images, UVec2::splat(16), default())],
        );
        retain_used(&mut atlases, &used);
        assert_eq!(atlases.len(), 1);
        assert_eq!(
            atlases[&key]
                .iter()
                .map(|page| page.texture.id())
                .collect::<Vec<_>>(),
            order
        );
        retain_used(&mut atlases, &HashSet::new());
        assert!(atlases.is_empty());
    }
}
