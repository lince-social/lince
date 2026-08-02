mod archive;
#[path = "communication/mod.rs"]
mod communication;
mod document_viewer;
mod freedoom;
#[path = "instinct/mod.rs"]
mod instinct;
#[path = "kanban/mod.rs"]
mod kanban;
#[path = "karma/mod.rs"]
mod karma;
mod lince_logo_led;
pub mod lynx_ui;
#[path = "ontology/mod.rs"]
mod ontology;
mod organ;
#[allow(dead_code)]
mod organ_management;
mod permissions;
mod record;
#[path = "relations/mod.rs"]
mod relations;
#[allow(dead_code)]
mod sand_publisher;
mod shell;
mod table;
mod terminal;
mod todo;
mod transfer;

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

#[allow(dead_code)]
enum OfficialWidgetBuilder {
    Html {
        feature_flag: &'static str,
        source_builder: SandSourceBuilder,
    },
    Package {
        feature_flag: &'static str,
        package_builder: fn() -> LincePackage,
    },
}

#[allow(dead_code)]
impl OfficialWidgetBuilder {
    fn feature_flag(&self) -> &'static str {
        match self {
            Self::Html { feature_flag, .. } | Self::Package { feature_flag, .. } => feature_flag,
        }
    }

    fn build_package(&self) -> LincePackage {
        match self {
            Self::Html { source_builder, .. } => render_widget(source_builder()),
            Self::Package {
                package_builder, ..
            } => package_builder(),
        }
    }
}

// Only current frame.js sands are wired for construction, plus `shell` (the
// board's own chrome). Legacy sources may remain under `sand/`, but stay
// unwired until rebuilt on the current bridge and explicitly added here.
const OFFICIAL_WIDGETS: [OfficialWidgetBuilder; 20] = [
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::edit_source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::zoom_source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::ai_source,
    },
    // The old flat `shell::tutorial_source` sand, rebuilt as a chaptered
    // package so it can carry mermaid diagrams (2026-08-01).
    OfficialWidgetBuilder::Package {
        feature_flag: instinct::FEATURE_FLAG,
        package_builder: instinct::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: document_viewer::FEATURE_FLAG,
        package_builder: document_viewer::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: freedoom::FEATURE_FLAG,
        package_builder: freedoom::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: lince_logo_led::FEATURE_FLAG,
        package_builder: lince_logo_led::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: terminal::FEATURE_FLAG,
        package_builder: terminal::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: kanban::FEATURE_FLAG,
        package_builder: kanban::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: relations::FEATURE_FLAG,
        package_builder: relations::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: ontology::FEATURE_FLAG,
        package_builder: ontology::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: record::FEATURE_FLAG,
        package_builder: record::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: organ::FEATURE_FLAG,
        package_builder: organ::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: table::FEATURE_FLAG,
        package_builder: table::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: permissions::FEATURE_FLAG,
        package_builder: permissions::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: todo::FEATURE_FLAG,
        package_builder: todo::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: transfer::FEATURE_FLAG,
        package_builder: transfer::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: karma::FEATURE_FLAG,
        package_builder: karma::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: archive::FEATURE_FLAG,
        package_builder: archive::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: communication::FEATURE_FLAG,
        package_builder: communication::package,
    },
];

pub fn official_packages() -> Result<Vec<LincePackage>, String> {
    Ok(OFFICIAL_WIDGETS
        .iter()
        .map(OfficialWidgetBuilder::build_package)
        .collect())
}

pub fn render_official_widgets(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|error| format!("Nao consegui criar ~/.config/lince/web/sand: {error}"))?;

    for builder in OFFICIAL_WIDGETS.iter() {
        let package = builder.build_package();
        let archive_filename = package.archive_filename();
        let bytes = build_lince_archive(&package)?;
        remove_package_variants(target_dir, &archive_filename, true)?;
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

fn remove_package_variants(
    target_dir: &Path,
    filename: &str,
    keep_expected: bool,
) -> Result<(), String> {
    let package_id = package_id_from_filename(filename);
    let expected = target_dir.join(filename);

    for extension in [
        PACKAGE_EXTENSION,
        LEGACY_PACKAGE_EXTENSION,
        LEGACY_PACKAGE_ARCHIVE_EXTENSION,
    ] {
        let candidate = target_dir.join(format!("{package_id}{extension}"));
        if (keep_expected && candidate == expected) || !candidate.exists() {
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

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn requested_current_way_sands_are_in_the_official_catalog() {
        let packages = official_packages().expect("build official sand catalog");
        for filename in [
            "document-viewer.lince",
            "freedoom-portal.lince",
            "lince-logo-led.html",
            "ghostty-terminal.lince",
        ] {
            let package = packages
                .iter()
                .find(|package| package.archive_filename() == filename)
                .unwrap_or_else(|| panic!("{filename} is missing from the official catalog"));
            assert!(
                package.html_document().contains("/board/frame.js"),
                "{filename} must use the current frame bridge"
            );
        }
    }
}
