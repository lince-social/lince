mod bucket_image_view;
mod calendar;
mod chess;
mod doom_portal;
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
#[path = "relations/mod.rs"]
mod relations;
mod sand_publisher;
#[path = "shared_markdown/mod.rs"]
mod shared_markdown;
mod spotify_control;
mod tasklist;
mod tasks_table;
mod trail;
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

pub(crate) use trail::{
    INLINE_STYLES as TRAIL_INLINE_STYLES, render_body as render_trail_body,
    render_script as render_trail_script,
};

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

const OFFICIAL_WIDGETS: [OfficialWidgetBuilder; 22] = [
    OfficialWidgetBuilder::Html {
        feature_flag: bucket_image_view::FEATURE_FLAG,
        source_builder: bucket_image_view::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: extra_simple::FEATURE_FLAG,
        source_builder: extra_simple::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: calendar::FEATURE_FLAG,
        source_builder: calendar::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: chess::FEATURE_FLAG,
        source_builder: chess::source,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: doom_portal::FEATURE_FLAG,
        package_builder: doom_portal::package,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: general_creation::FEATURE_FLAG,
        source_builder: general_creation::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: kanban_record_view::FEATURE_FLAG,
        source_builder: kanban_record_view::source,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: relations::FEATURE_FLAG,
        package_builder: relations::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: trail::FEATURE_FLAG,
        package_builder: trail::package,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: lince_logo_led::FEATURE_FLAG,
        source_builder: lince_logo_led::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: link_chip::FEATURE_FLAG,
        source_builder: link_chip::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: local_terminal::FEATURE_FLAG,
        source_builder: local_terminal::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: markdown_notes::FEATURE_FLAG,
        source_builder: markdown_notes::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: organ_management::FEATURE_FLAG,
        source_builder: organ_management::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: ops_clock::FEATURE_FLAG,
        source_builder: ops_clock::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: view_table_editor::FEATURE_FLAG,
        source_builder: view_table_editor::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: sand_publisher::FEATURE_FLAG,
        source_builder: sand_publisher::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: spotify_control::FEATURE_FLAG,
        source_builder: spotify_control::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: record_crud::FEATURE_FLAG,
        source_builder: record_crud::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: tasklist::FEATURE_FLAG,
        source_builder: tasklist::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: tasks_table::FEATURE_FLAG,
        source_builder: tasks_table::source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: weather::FEATURE_FLAG,
        source_builder: weather::source,
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
