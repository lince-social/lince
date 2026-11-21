// non-lean-yet
// Scope: mod fallback_image, mod gpu_image, mod manual_texture_view, mod texture_attachment.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 61.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 63.
// - [Clippy: clippy::redundant_clone] Move or borrow at the marked sites and check later uses.
//   Sites 66.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 26.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 26.
// - [review: §9 Modules] Check the intended public API and re-export individual items where
//   possible. Sites 9, 10, 11, 12, 13.

mod fallback_image;
mod gpu_image;
mod manual_texture_view;
mod texture_attachment;
mod texture_cache;

pub use crate::render_resource::DefaultImageSampler;
use bevy_image::{CompressedImageFormatSupport, CompressedImageFormats, ImageLoader, ImagePlugin};
pub use fallback_image::*;
pub use gpu_image::*;
pub use manual_texture_view::*;
pub use texture_attachment::*;
pub use texture_cache::*;

use crate::{
    extract_resource::ExtractResourcePlugin, init_gpu_resource, render_asset::RenderAssetPlugin,
    render_resource::DefaultImageSamplerDescriptor, GpuResourceAppExt, Render, RenderApp,
    RenderStartup, RenderSystems,
};
use bevy_app::{App, Plugin};
use bevy_asset::AssetApp;
use bevy_ecs::prelude::*;
use bevy_log::warn;

#[derive(Default)]
pub struct TexturePlugin;

impl Plugin for TexturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            RenderAssetPlugin::<GpuImage>::default(),
            ExtractResourcePlugin::<ManualTextureViews>::default(),
        ))
        .init_resource::<ManualTextureViews>();
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ManualTextureViews>()
                .init_gpu_resource::<TextureCache>()
                .allow_ambiguous_resource::<TextureCache>()
                .add_systems(
                    Render,
                    update_texture_cache_system.in_set(RenderSystems::Cleanup),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        if !ImageLoader::SUPPORTED_FORMATS.is_empty() {
            let supported_compressed_formats = if let Some(resource) =
                app.world().get_resource::<CompressedImageFormatSupport>()
            {
                resource.0
            } else {
                warn!("CompressedImageFormatSupport resource not found. It should either be initialized in finish() of \
                       RenderPlugin, or manually if not using the RenderPlugin or the WGPU backend.");
                CompressedImageFormats::NONE
            };

            app.register_asset_loader(ImageLoader::new(supported_compressed_formats));
        }
        let default_sampler = app.get_added_plugins::<ImagePlugin>()[0]
            .default_sampler
            .clone();

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(DefaultImageSamplerDescriptor(default_sampler.clone()));
            render_app.add_systems(
                RenderStartup,
                (
                    init_gpu_resource::<DefaultImageSampler>,
                    init_gpu_resource::<FallbackImage>,
                    init_gpu_resource::<FallbackImageZero>,
                    init_gpu_resource::<FallbackImageCubemap>,
                    init_gpu_resource::<FallbackImageFormatMsaaCache>,
                )
                    .chain()
                    .ambiguous_with_all(),
            );
        }
    }
}
