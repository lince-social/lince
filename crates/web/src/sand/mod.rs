#[allow(dead_code)]
mod calendar;
#[allow(dead_code)]
mod chess;
#[allow(dead_code)]
mod document_viewer;
#[path = "example-bundle/mod.rs"]
mod example_bundle;
#[allow(dead_code)]
mod finance;
#[allow(dead_code)]
mod freedoom;
#[allow(dead_code)]
mod home_manager;
#[path = "kanban/mod.rs"]
mod kanban;
#[allow(dead_code)]
mod karma_orchestra;
#[allow(dead_code)]
mod lince_logo_led;
#[allow(dead_code)]
#[path = "markdown_notes/mod.rs"]
mod markdown_notes;
#[allow(dead_code)]
mod ops_clock;
#[allow(dead_code)]
mod organ_management;
#[allow(dead_code)]
pub(crate) mod record_editor;
mod record_info;
#[path = "relations/mod.rs"]
mod relations;
#[allow(dead_code)]
mod role_access;
#[allow(dead_code)]
mod sand_publisher;
#[allow(dead_code)]
#[path = "shared_markdown/mod.rs"]
mod shared_markdown;
mod shell;
#[allow(dead_code)]
mod spotify_control;
mod table;
#[allow(dead_code)]
mod terminal;
mod todo;
#[allow(dead_code)]
mod transfer;
#[allow(dead_code)]
mod weather;

use {
    crate::domain::{
        board::{BoardCard, BoardWorkspace, default_camera},
        lince_package::{
            LEGACY_PACKAGE_ARCHIVE_EXTENSION, LEGACY_PACKAGE_EXTENSION, LincePackage,
            PACKAGE_EXTENSION, PackageManifest, build_lince_archive, package_id_from_filename,
        },
        workspace_archive::build_workspace_archive,
    },
    maud::{DOCTYPE, Markup, PreEscaped, html},
    serde_json::{Map, Value},
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

// Stage 8b: only the NEW-WAY sands (self-contained `.html` strings that connect
// via `/board/frame.js` + the transport WebSocket) are wired for construction,
// plus `shell` (the board's own chrome). Every other sand still depends on the
// old host/maud way and is RETIRED from wiring — its module + source files stay
// on disk (see the `mod` list above) to be rebuilt into the new way later, but
// it is not built, emitted, or served. Re-add an entry here once migrated.
const OFFICIAL_WIDGETS: [OfficialWidgetBuilder; 14] = [
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::logo_source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::operation_source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::workspaces_source,
    },
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::notifications_source,
    },
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
    OfficialWidgetBuilder::Html {
        feature_flag: shell::FEATURE_FLAG,
        source_builder: shell::tutorial_source,
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
        feature_flag: record_info::FEATURE_FLAG,
        package_builder: record_info::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: table::FEATURE_FLAG,
        package_builder: table::package,
    },
    OfficialWidgetBuilder::Package {
        feature_flag: todo::FEATURE_FLAG,
        package_builder: todo::package,
    },
    // Reference directory-bundle sand (base task 3): ships as `example-bundle.lince`.
    OfficialWidgetBuilder::Package {
        feature_flag: example_bundle::FEATURE_FLAG,
        package_builder: example_bundle::package,
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

// --- Sand-as-group packaging (Stage 8b, Phase 3) ---------------------------
// Some sands ship not as a single card but as a GROUP of sub-sands with a
// relative layout + z-order and a shared inner group id, exported as a
// `.group.sand` workspace archive. Importing it drops the whole group onto the
// board at once (see `import_group`), and the shared inner group both keeps the
// sub-sands together under nesting and scopes their ABI events to each other.

/// Turn a rendered sand package into a board card at the given rect / z-order,
/// tagged with the group stack and ABI listen topics.
fn card_from_package(
    package: &LincePackage,
    id: &str,
    rect: (f64, f64, f64, f64),
    z_index: i32,
    group_ids: Vec<String>,
    abi_listen: Vec<String>,
) -> BoardCard {
    let (x, y, width, height) = rect;
    BoardCard {
        id: id.to_string(),
        kind: "package".into(),
        title: package.manifest.title.clone(),
        description: package.manifest.description.clone(),
        text: String::new(),
        html: package.html.clone(),
        author: package.manifest.author.clone(),
        permissions: package.manifest.permissions.clone(),
        package_name: package.archive_filename(),
        requires_server: package.manifest.requires_server,
        server_id: String::new(),
        view_id: None,
        streams_enabled: true,
        widget_state: Value::Object(Map::new()),
        x,
        y,
        width,
        height,
        pinned: false,
        system: false,
        z_index,
        group_id: group_ids.last().cloned(),
        group_ids,
        abi_listen,
    }
}

/// Build the default kanban GROUP: the kanban board plus a record_info sand
/// BESIDE it (to the right), sharing one inner group id. Both are visible so the
/// group reads as a group and the board stays usable; clicking a kanban card
/// scopes a `recordClicked` to this record_info, which then shows that record.
/// (An earlier design stacked record_info at the SAME rect on top, but an opaque
/// iframe there just hid the board — hence side-by-side.) Disbanding an outer
/// group later leaves the pair intact (groupception). Returns a `.group.sand`
/// archive.
pub fn build_kanban_group_archive() -> Result<Vec<u8>, String> {
    let kanban = kanban::package();
    let record_info = record_info::package();

    let inner_group = format!("group-kanban-{}", package_id_from_filename("kanban"));
    let board_rect = (49_000.0, 49_000.0, 720.0, 520.0);
    // record_info sits immediately to the right of the board.
    let info_rect = (49_000.0 + 720.0 + 16.0, 49_000.0, 320.0, 520.0);

    let cards = vec![
        card_from_package(
            &kanban,
            "card-kanban",
            board_rect,
            1,
            vec![inner_group.clone()],
            Vec::new(),
        ),
        card_from_package(
            &record_info,
            "card-kanban-record-info",
            info_rect,
            2,
            vec![inner_group],
            vec!["recordClicked".into()],
        ),
    ];

    let workspace = BoardWorkspace {
        id: "kanban-group".into(),
        name: "Kanban".into(),
        camera: default_camera(),
        cards,
    };

    build_workspace_archive(&workspace, &[kanban, record_info])
}

/// Emit the official sand-GROUP archives (currently: kanban) into the sand dir,
/// alongside the single-sand packages from `render_official_widgets`.
pub fn render_official_groups(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|error| format!("Nao consegui criar ~/.config/lince/web/sand: {error}"))?;
    // Groups ship as `.lince` workspace archives (a group of sub-sands with
    // layout + z-order). The catalog `list()` skips workspace archives by peeking
    // for `workspace.json`, so this `.lince` is not mistaken for a single sand.
    let filename = "kanban.lince";
    let bytes = build_kanban_group_archive()?;
    let path = target_dir.join(filename);
    std::fs::write(&path, bytes)
        .map_err(|error| format!("Nao consegui escrever {}: {error}", path.display()))?;
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
mod group_tests {
    use super::*;
    use crate::domain::workspace_archive::parse_workspace_archive;

    #[test]
    fn kanban_ships_as_a_group_of_board_plus_record_info() {
        let bytes = build_kanban_group_archive().expect("build kanban group archive");
        assert!(
            crate::domain::workspace_archive::is_workspace_archive_bytes(&bytes),
            "group archive is detectable by content so the catalog skips it",
        );
        let imported = parse_workspace_archive("kanban.lince", &bytes)
            .expect("parse kanban group archive");

        let cards = &imported.workspace.cards;
        assert_eq!(cards.len(), 2, "kanban group is exactly board + record_info");

        let board = &cards[0];
        let info = &cards[1];

        // Both sub-sands share ONE inner group (keeps them together + scopes ABI).
        assert_eq!(board.group_ids.len(), 1);
        assert_eq!(board.group_ids, info.group_ids, "shared inner group id");
        assert_eq!(board.group_id, board.group_ids.last().cloned());

        // record_info sits BESIDE the board (to its right), not covering it, so
        // both are visible as a group and the board stays usable.
        assert!(info.x >= board.x + board.width, "record_info is right of the board");
        assert_eq!(board.y, info.y, "record_info shares the board's top edge");
        assert!(info.z_index > board.z_index, "record_info is above the board");

        // Only record_info listens for the board's recordClicked event.
        assert_eq!(info.abi_listen, vec!["recordClicked".to_string()]);
        assert!(board.abi_listen.is_empty());

        // Both packages travel in the archive so the import is self-contained.
        assert_eq!(imported.packages.len(), 2);
    }
}
