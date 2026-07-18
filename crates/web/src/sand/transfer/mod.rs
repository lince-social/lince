mod body;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    maud::{DOCTYPE, Markup, html},
    std::collections::BTreeMap,
};

pub(crate) const FEATURE_FLAG: &str = "sand.transfer";

const STYLES_CSS: &str = include_str!("styles.css");
const APP_MAIN_JS: &str = include_str!("app/main.js");
const APP_MODEL_JS: &str = include_str!("app/model.js");
const APP_RENDER_JS: &str = include_str!("app/render.js");
const APP_CREATE_JS: &str = include_str!("app/create.js");
const APP_CREATE_MODEL_JS: &str = include_str!("app/create-model.js");

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "⇄".into(),
        title: "Transfers".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Create and inspect transfer commitments from the whole portfolio down to one promise."
            .into(),
        details: "A live Transfer surface. The overview preserves agreement, hierarchy, party, promise, balance, and confirmation context; a guided composer reviews public terms and local quantity effects before committing typed Actions.".into(),
        initial_width: 8,
        initial_height: 6,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    };

    let mut assets = BTreeMap::new();
    assets.insert("styles.css".into(), STYLES_CSS.as_bytes().to_vec());
    assets.insert("app/main.js".into(), APP_MAIN_JS.as_bytes().to_vec());
    assets.insert("app/model.js".into(), APP_MODEL_JS.as_bytes().to_vec());
    assets.insert("app/render.js".into(), APP_RENDER_JS.as_bytes().to_vec());
    assets.insert("app/create.js".into(), APP_CREATE_JS.as_bytes().to_vec());
    assets.insert(
        "app/create-model.js".into(),
        APP_CREATE_MODEL_JS.as_bytes().to_vec(),
    );

    LincePackage::new_archive(
        Some("transfers.lince".into()),
        manifest.clone(),
        document(&manifest),
        "index.html",
        assets,
    )
    .expect("transfer sand should render as a valid archive package")
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
    fn package_is_a_live_transfer_creation_and_inspection_surface() {
        let package = package();
        let html = package.html_document();
        let assets = package.asset_paths().collect::<Vec<_>>();

        assert!(html.contains("/board/frame.js"));
        assert!(html.contains("app/main.js"));
        assert!(assets.contains(&"app/model.js"));
        assert!(assets.contains(&"app/render.js"));
        assert!(assets.contains(&"app/create.js"));
        assert!(assets.contains(&"app/create-model.js"));
        assert_eq!(
            package.manifest.permissions,
            vec![
                "bridge_state".to_string(),
                "protein_subscribe".to_string(),
                "act".to_string(),
            ]
        );
    }
}
