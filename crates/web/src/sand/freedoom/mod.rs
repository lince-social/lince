mod body;
mod script;
mod style;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    maud::{DOCTYPE, PreEscaped, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.doom_portal";

const ENGINE_JS: &[u8] = include_bytes!("vendor/websockets-doom.js");
const ENGINE_WASM: &[u8] = include_bytes!("vendor/websockets-doom.wasm");
const FREEDOOM_WAD: &[u8] = include_bytes!("vendor/doom1.wad");
const COPYING_TEXT: &str = include_str!("vendor/COPYING.txt");
const CREDITS_TEXT: &str = include_str!("vendor/CREDITS.txt");
const DEFAULT_CFG: &str = r#"mouse_sensitivity 8
show_messages 1
screenblocks 10
detaillevel 0
sfx_volume 8
music_volume 0
use_mouse 1
mouseb_fire 0
mouseb_strafe 1
mouseb_forward 2
key_right 0xae
key_left 0xac
key_up 0xad
key_down 0xaf
use_joystick 0
"#;

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "D".into(),
        title: "Freedoom Portal".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
        description: "Self-contained Freedoom sand with local wasm, local WAD data, and no remote iframe dependency.".into(),
        details: "Starts a local Freedoom Phase 1 session without Records or network game state. The archive carries the wasm runtime, WAD, configuration, license, and credits.".into(),
        initial_width: 6,
        initial_height: 6,
        requires_server: false,
        permissions: vec![],
    };

    let mut assets = BTreeMap::new();
    assets.insert("websockets-doom.js".into(), ENGINE_JS.to_vec());
    assets.insert("websockets-doom.wasm".into(), ENGINE_WASM.to_vec());
    assets.insert("doom1.wad".into(), FREEDOOM_WAD.to_vec());
    assets.insert("default.cfg".into(), DEFAULT_CFG.as_bytes().to_vec());
    assets.insert("COPYING.txt".into(), COPYING_TEXT.as_bytes().to_vec());
    assets.insert("CREDITS.txt".into(), CREDITS_TEXT.as_bytes().to_vec());

    LincePackage::new_archive(
        Some("freedoom-portal.lince".into()),
        manifest,
        document(),
        "index.html",
        assets,
    )
    .expect("freedoom official sand should render as a valid archive package")
}

fn document() -> String {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Freedoom Portal" }
                script src="/board/frame.js" {}
                style { (PreEscaped(style::CSS)) }
            }
            body {
                (body::body())
                script { (PreEscaped(script::BOOTSTRAP)) }
                script src="websockets-doom.js" {}
            }
        }
    }
    .into_string()
}
