mod body;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    maud::{DOCTYPE, Markup, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.ghostty_terminal";

const STYLES_CSS: &str = include_str!("styles.css");
const APP_MAIN_JS: &str = include_str!("app/main.js");
const APP_HOST_SESSION_JS: &str = include_str!("app/host-session.js");
const APP_KEYMAP_JS: &str = include_str!("app/keymap.js");
const APP_GHOSTTY_RUNTIME_JS: &str = include_str!("app/ghostty-runtime.js");
const APP_QUERY_REPLIES_JS: &str = include_str!("app/query-replies.js");
const GHOSTTY_WASM: &[u8] = include_bytes!("vendor/ghostty-vt.wasm");
const LICENSE_TEXT: &str = include_str!("vendor/LICENSE.txt");
const UPSTREAM_TEXT: &str = include_str!("vendor/UPSTREAM.txt");

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "G".into(),
        title: "Ghostty Terminal".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
        description: "Browser terminal sand powered by Ghostty's VT core compiled to WebAssembly.".into(),
        details: "Bundles libghostty-vt as a local wasm asset, parses terminal bytes in the browser, and exchanges ephemeral PTY bytes through the board host bridge without opening a sand-owned WebSocket.".into(),
        initial_width: 6,
        initial_height: 5,
        requires_server: false,
        permissions: vec!["terminal_session".into()],
    };

    let mut assets = BTreeMap::new();
    assets.insert("styles.css".into(), STYLES_CSS.as_bytes().to_vec());
    assets.insert("app/main.js".into(), APP_MAIN_JS.as_bytes().to_vec());
    assets.insert(
        "app/host-session.js".into(),
        APP_HOST_SESSION_JS.as_bytes().to_vec(),
    );
    assets.insert("app/keymap.js".into(), APP_KEYMAP_JS.as_bytes().to_vec());
    assets.insert(
        "app/ghostty-runtime.js".into(),
        APP_GHOSTTY_RUNTIME_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/query-replies.js".into(),
        APP_QUERY_REPLIES_JS.as_bytes().to_vec(),
    );
    assets.insert("vendor/ghostty-vt.wasm".into(), GHOSTTY_WASM.to_vec());
    assets.insert(
        "vendor/LICENSE.txt".into(),
        LICENSE_TEXT.as_bytes().to_vec(),
    );
    assets.insert(
        "vendor/UPSTREAM.txt".into(),
        UPSTREAM_TEXT.as_bytes().to_vec(),
    );

    LincePackage::new_archive(
        Some("ghostty-terminal.lince".into()),
        manifest.clone(),
        document(&manifest),
        "index.html",
        assets,
    )
    .expect("ghostty terminal sand should render as a valid archive package")
}

fn document(manifest: &PackageManifest) -> String {
    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (manifest.title.as_str()) }
                link rel="stylesheet" href="styles.css";
                script src="/board/frame.js" {}
            }
            body {
                (body::body())
                script type="module" src="app/main.js" {}
            }
        }
    };

    markup.into_string()
}

#[cfg(test)]
mod tests {
    use super::package;

    #[test]
    fn package_uses_the_board_terminal_host_and_bundles_ghostty_notices() {
        let package = package();
        let html = package.html_document();
        let assets = package.asset_paths().collect::<Vec<_>>();

        assert!(html.contains("/board/frame.js"));
        assert!(html.contains("app/main.js"));
        assert!(assets.contains(&"app/host-session.js"));
        assert!(assets.contains(&"vendor/ghostty-vt.wasm"));
        assert!(assets.contains(&"vendor/LICENSE.txt"));
        assert!(assets.contains(&"vendor/UPSTREAM.txt"));
        assert!(!assets.iter().any(|path| path.contains("socket.js")));
        assert!(
            package
                .manifest
                .permissions
                .contains(&"terminal_session".into())
        );
    }
}
