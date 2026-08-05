use {
    crate::domain::lince_package::{LincePackage, PackageManifest, build_lince_archive},
    std::{collections::BTreeMap, path::Path},
};

const HTML: &str = include_str!("index.html");
const DEMO_CSS: &[u8] = include_bytes!("demo.css");
const DEMO_JS: &[u8] = include_bytes!("demo.js");
const LYNX_UI_CSS: &[u8] = include_bytes!("../../../static/presentation/board/lynx-ui.css");
const LYNX_UI_JS: &[u8] = include_bytes!("../../../static/presentation/board/lynx-ui.js");
const CATPPUCCIN_MACCHIATO: &[u8] = include_bytes!("styles/catppuccin-macchiato.css");
const CATPPUCCIN_LICENSE: &[u8] = include_bytes!("styles/CATPPUCCIN-LICENSE.txt");
const LATO_REGULAR: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/Lato-Regular.ttf");
const LATO_BOLD: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/Lato-Bold.ttf");
const LATO_ITALIC: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/Lato-Italic.ttf");
const ALEO_VARIABLE: &[u8] =
    include_bytes!("../../../../../assets/fonts/Aleo/Aleo-VariableFont_wght.ttf");
const LATO_LICENSE: &[u8] = include_bytes!("../../../../../assets/fonts/Lato/OFL.txt");
const ALEO_LICENSE: &[u8] = include_bytes!("../../../../../assets/fonts/Aleo/OFL.txt");

fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "◇".into(),
        title: "LynxUI Gallery".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Development gallery for every LynxUI component.".into(),
        details: "Kanban, messaging, inventory, and request workflows backed by the canonical LynxUI components, icons, and Lynx colorscheme tokens.".into(),
        initial_width: 8,
        initial_height: 7,
        requires_server: false,
        permissions: vec![],
    }
}

pub fn package() -> LincePackage {
    let assets = BTreeMap::from([
        ("lynx-ui.css".into(), LYNX_UI_CSS.to_vec()),
        ("lynx-ui.js".into(), LYNX_UI_JS.to_vec()),
        ("demo.css".into(), DEMO_CSS.to_vec()),
        ("demo.js".into(), DEMO_JS.to_vec()),
        (
            "styles/catppuccin-macchiato.css".into(),
            CATPPUCCIN_MACCHIATO.to_vec(),
        ),
        (
            "styles/CATPPUCCIN-LICENSE.txt".into(),
            CATPPUCCIN_LICENSE.to_vec(),
        ),
        ("fonts/Lato-Regular.ttf".into(), LATO_REGULAR.to_vec()),
        ("fonts/Lato-Bold.ttf".into(), LATO_BOLD.to_vec()),
        ("fonts/Lato-Italic.ttf".into(), LATO_ITALIC.to_vec()),
        (
            "fonts/Aleo-VariableFont_wght.ttf".into(),
            ALEO_VARIABLE.to_vec(),
        ),
        ("fonts/Lato-OFL.txt".into(), LATO_LICENSE.to_vec()),
        ("fonts/Aleo-OFL.txt".into(), ALEO_LICENSE.to_vec()),
    ]);
    LincePackage::new_archive(
        Some("lynx-ui-gallery.lince".into()),
        manifest(),
        HTML,
        "index.html",
        assets,
    )
    .expect("LynxUI gallery should render as a valid archive package")
}

pub fn render(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|error| format!("Unable to create {}: {error}", target_dir.display()))?;
    let package = package();
    let target = target_dir.join(package.archive_filename());
    std::fs::write(&target, build_lince_archive(&package)?)
        .map_err(|error| format!("Unable to write {}: {error}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_bundles_the_canonical_library_and_demo() {
        let package = package();
        for path in [
            "lynx-ui.css",
            "lynx-ui.js",
            "demo.css",
            "demo.js",
            "styles/catppuccin-macchiato.css",
            "styles/CATPPUCCIN-LICENSE.txt",
            "fonts/Lato-Regular.ttf",
            "fonts/Lato-Bold.ttf",
            "fonts/Lato-Italic.ttf",
            "fonts/Aleo-VariableFont_wght.ttf",
            "fonts/Lato-OFL.txt",
            "fonts/Aleo-OFL.txt",
        ] {
            assert!(package.asset_bytes(path).is_some(), "missing {path}");
        }
        assert!(package.html_document().contains("class=\"lynx-ui\""));

        let demo = std::str::from_utf8(DEMO_JS).expect("demo JavaScript should be UTF-8");
        for component in [
            "lynx-button-group",
            "lynx-input",
            "lynx-textarea",
            "lynx-select",
            "lynx-check",
            "lynx-radio",
            "lynx-label",
            "lynx-help",
            "lynx-error",
            "lynx-box",
            "lynx-panel",
            "lynx-stack",
            "lynx-row",
            "lynx-grid",
            "lynx-toolbar",
            "lynx-divider",
            "lynx-status",
            "lynx-callout",
            "lynx-empty",
            "lynx-table",
            "lynx-list",
            "lynx-dropdown",
            "data-lynx-tooltip",
            "lynx-dialog",
            "lynx-tabs",
            "lynx-disclosure",
            "show-toast",
        ] {
            assert!(demo.contains(component), "gallery does not use {component}");
        }

        let library = std::str::from_utf8(LYNX_UI_JS).expect("LynxUI JavaScript should be UTF-8");
        assert!(library.contains("global.LynxUI"));
        assert!(library.contains("inspect"));
        assert!(library.contains("function toast"));
        assert!(library.contains("trash:"));
    }
}
