mod bucket_image_view;
mod calendar;
mod chess;
mod extra_simple;
mod general_creation;
#[path = "kanban_record_view/mod.rs"]
mod kanban_record_view;
mod lince_logo_led;
mod link_chip;
mod local_terminal;
#[path = "markdown_notes/mod.rs"]
mod markdown_notes;
mod ops_clock;
mod organ_management;
mod record_crud;
mod sand_publisher;
#[path = "shared_markdown/mod.rs"]
mod shared_markdown;
mod spotify_control;
mod tasklist;
mod tasks_table;
mod doom_portal;
mod view_table_editor;
mod weather;

use {
    crate::domain::lince_package::{
        LEGACY_PACKAGE_ARCHIVE_EXTENSION, LEGACY_PACKAGE_EXTENSION, LincePackage,
        PACKAGE_EXTENSION, PackageManifest, build_lince_archive, package_id_from_filename,
    },
    maud::{DOCTYPE, Markup, PreEscaped, html},
    std::path::Path,
};

pub(crate) struct HeadLink {
    pub(crate) rel: &'static str,
    pub(crate) href: &'static str,
}

pub(crate) enum WidgetScript {
    Src(&'static str),
    Inline(String),
}

impl WidgetScript {
    pub(crate) fn src(value: &'static str) -> Self {
        Self::Src(value)
    }

    pub(crate) fn inline(value: impl Into<String>) -> Self {
        Self::Inline(value.into())
    }
}

pub(crate) struct SandWidgetSource {
    pub(crate) filename: &'static str,
    pub(crate) lang: &'static str,
    pub(crate) manifest: PackageManifest,
    pub(crate) head_links: Vec<HeadLink>,
    pub(crate) inline_styles: Vec<&'static str>,
    pub(crate) body: Markup,
    pub(crate) body_scripts: Vec<WidgetScript>,
}

type SandSourceBuilder = fn() -> SandWidgetSource;

enum OfficialWidgetBuilder {
    Html(SandSourceBuilder),
    Package(fn() -> LincePackage),
}

const OFFICIAL_WIDGETS: [OfficialWidgetBuilder; 20] = [
    OfficialWidgetBuilder::Html(bucket_image_view::source),
    OfficialWidgetBuilder::Html(extra_simple::source),
    OfficialWidgetBuilder::Html(calendar::source),
    OfficialWidgetBuilder::Html(chess::source),
    OfficialWidgetBuilder::Package(doom_portal::package),
    OfficialWidgetBuilder::Html(general_creation::source),
    OfficialWidgetBuilder::Html(kanban_record_view::source),
    OfficialWidgetBuilder::Html(lince_logo_led::source),
    OfficialWidgetBuilder::Html(link_chip::source),
    OfficialWidgetBuilder::Html(local_terminal::source),
    OfficialWidgetBuilder::Html(markdown_notes::source),
    OfficialWidgetBuilder::Html(organ_management::source),
    OfficialWidgetBuilder::Html(ops_clock::source),
    OfficialWidgetBuilder::Html(view_table_editor::source),
    OfficialWidgetBuilder::Html(sand_publisher::source),
    OfficialWidgetBuilder::Html(spotify_control::source),
    OfficialWidgetBuilder::Html(record_crud::source),
    OfficialWidgetBuilder::Html(tasklist::source),
    OfficialWidgetBuilder::Html(tasks_table::source),
    OfficialWidgetBuilder::Html(weather::source),
];

pub fn official_packages() -> Vec<LincePackage> {
    OFFICIAL_WIDGETS
        .into_iter()
        .map(|builder| match builder {
            OfficialWidgetBuilder::Html(source_builder) => render_widget(source_builder()),
            OfficialWidgetBuilder::Package(package_builder) => package_builder(),
        })
        .collect()
}

pub fn render_official_widgets(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|error| format!("Nao consegui criar ~/.config/lince/web/sand: {error}"))?;

    for package in official_packages() {
        let bytes = build_lince_archive(&package)?;
        let archive_filename = package.archive_filename();
        remove_stale_package_variants(target_dir, &archive_filename)?;
        let path = target_dir.join(&archive_filename);
        std::fs::write(&path, bytes)
            .map_err(|error| format!("Nao consegui escrever {}: {error}", path.display()))?;
    }

    Ok(())
}

fn render_widget(source: SandWidgetSource) -> LincePackage {
    let SandWidgetSource {
        filename,
        lang,
        manifest,
        head_links,
        inline_styles,
        body,
        body_scripts,
    } = source;
    let document_title = manifest.title.clone();

    let markup = html! {
        (DOCTYPE)
        html lang=(lang) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (document_title) }
                @for link in &head_links {
                    link rel=(link.rel) href=(link.href);
                }
                @for style_block in &inline_styles {
                    style { (PreEscaped(style_block)) }
                }
            }
            body {
                (body)
                @for script in &body_scripts {
                    @match script {
                        WidgetScript::Src(src) => {
                            script src=(src) {}
                        }
                        WidgetScript::Inline(source) => {
                            script { (PreEscaped(source.as_str())) }
                        }
                    }
                }
            }
        }
    }
    .into_string();

    LincePackage::new(Some(filename.to_string()), manifest, markup)
        .expect("official sand widget should render as valid HTML")
}

fn remove_stale_package_variants(target_dir: &Path, filename: &str) -> Result<(), String> {
    let package_id = package_id_from_filename(filename);
    let expected = target_dir.join(filename);

    for extension in [
        PACKAGE_EXTENSION,
        LEGACY_PACKAGE_EXTENSION,
        LEGACY_PACKAGE_ARCHIVE_EXTENSION,
    ] {
        let candidate = target_dir.join(format!("{package_id}{extension}"));
        if candidate == expected || !candidate.exists() {
            continue;
        }

        std::fs::remove_file(&candidate).map_err(|error| {
            format!(
                "Nao consegui limpar a versao antiga de {}: {error}",
                candidate.display()
            )
        })?;
    }

    Ok(())
}
