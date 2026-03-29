mod bucket_image_view;
mod extra_simple;
mod calendar;
mod general_creation;
mod kanban_record_view;
mod lince_logo_led;
mod link_chip;
mod local_terminal;
#[path = "markdown_notes/mod.rs"]
mod markdown_notes;
mod organ_management;
mod ops_clock;
mod view_table_editor;
mod spotify_control;
mod record_crud;
mod tasklist;
mod tasks_table;
mod weather;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest, build_lince_archive},
    maud::{DOCTYPE, Markup, PreEscaped, html},
    std::path::Path,
};

pub(crate) struct HeadLink {
    pub(crate) rel: &'static str,
    pub(crate) href: &'static str,
}

pub(crate) enum WidgetScript {
    Src(&'static str),
    Inline(&'static str),
}

impl WidgetScript {
    pub(crate) fn src(value: &'static str) -> Self {
        Self::Src(value)
    }

    pub(crate) fn inline(value: &'static str) -> Self {
        Self::Inline(value)
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

const OFFICIAL_WIDGETS: [SandSourceBuilder; 17] = [
    bucket_image_view::source,
    extra_simple::source,
    calendar::source,
    general_creation::source,
    kanban_record_view::source,
    lince_logo_led::source,
    link_chip::source,
    local_terminal::source,
    markdown_notes::source,
    organ_management::source,
    ops_clock::source,
    view_table_editor::source,
    spotify_control::source,
    record_crud::source,
    tasklist::source,
    tasks_table::source,
    weather::source,
];

pub fn official_packages() -> Vec<LincePackage> {
    OFFICIAL_WIDGETS
        .into_iter()
        .map(|builder| render_widget(builder()))
        .collect()
}

pub fn render_official_widgets(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|error| format!("Nao consegui criar ~/.config/lince/web/sand: {error}"))?;

    let packages = official_packages();
    let expected = packages
        .iter()
        .map(LincePackage::archive_filename)
        .collect::<std::collections::BTreeSet<_>>();

    for package in &packages {
        let bytes = build_lince_archive(package)?;
        let path = target_dir.join(package.archive_filename());
        std::fs::write(&path, bytes)
            .map_err(|error| format!("Nao consegui escrever {}: {error}", path.display()))?;
    }

    for entry in std::fs::read_dir(target_dir)
        .map_err(|error| format!("Nao consegui ler ~/.config/lince/web/sand: {error}"))?
    {
        let entry = entry.map_err(|error| {
            format!("Nao consegui ler um item de ~/.config/lince/web/sand: {error}")
        })?;
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !filename.ends_with(".html") || expected.contains(filename) {
            continue;
        }
        std::fs::remove_file(&path)
            .map_err(|error| format!("Nao consegui limpar {}: {error}", path.display()))?;
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
                            script { (PreEscaped(source)) }
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
