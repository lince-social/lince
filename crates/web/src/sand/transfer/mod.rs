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
const APP_PRESETS_JS: &str = include_str!("app/presets.js");
const APP_AGREEMENT_JS: &str = include_str!("app/agreement.js");
const APP_NEGOTIATION_JS: &str = include_str!("app/negotiation.js");
const APP_OCCURRENCE_JS: &str = include_str!("app/occurrence.js");
const APP_HIERARCHY_JS: &str = include_str!("app/hierarchy.js");
const APP_BULK_JS: &str = include_str!("app/bulk.js");
const APP_DELIVERY_JS: &str = include_str!("app/delivery.js");
const APP_DELIVERY_MODEL_JS: &str = include_str!("app/delivery/model.js");
const APP_DELIVERY_RECIPIENTS_JS: &str = include_str!("app/delivery/recipients.js");
const APP_DELIVERY_RECEIPTS_JS: &str = include_str!("app/delivery/receipts.js");
const APP_DELIVERY_CONFLICTS_JS: &str = include_str!("app/delivery/conflicts.js");
const APP_INSPECTION_JS: &str = include_str!("app/inspection.js");
const APP_INSPECTION_ACCOUNTING_JS: &str = include_str!("app/inspection/accounting.js");
const APP_INSPECTION_CORRECTIONS_JS: &str = include_str!("app/inspection/corrections.js");
const APP_INSPECTION_DISCLOSURE_JS: &str = include_str!("app/inspection/disclosure.js");
const APP_INSPECTION_PROOF_DRAWER_JS: &str = include_str!("app/inspection/proof-drawer.js");
const APP_INSPECTION_SHARED_JS: &str = include_str!("app/inspection/shared.js");
const APP_INSPECTION_TIMELINE_JS: &str = include_str!("app/inspection/timeline.js");

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "⇄".into(),
        title: "Transfers".into(),
        author: "Lince Labs".into(),
        version: "0.3.0".into(),
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
    assets.insert("app/presets.js".into(), APP_PRESETS_JS.as_bytes().to_vec());
    assets.insert(
        "app/agreement.js".into(),
        APP_AGREEMENT_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/negotiation.js".into(),
        APP_NEGOTIATION_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/occurrence.js".into(),
        APP_OCCURRENCE_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/hierarchy.js".into(),
        APP_HIERARCHY_JS.as_bytes().to_vec(),
    );
    assets.insert("app/bulk.js".into(), APP_BULK_JS.as_bytes().to_vec());
    assets.insert(
        "app/delivery.js".into(),
        APP_DELIVERY_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/delivery/model.js".into(),
        APP_DELIVERY_MODEL_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/delivery/recipients.js".into(),
        APP_DELIVERY_RECIPIENTS_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/delivery/receipts.js".into(),
        APP_DELIVERY_RECEIPTS_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/delivery/conflicts.js".into(),
        APP_DELIVERY_CONFLICTS_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection.js".into(),
        APP_INSPECTION_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/accounting.js".into(),
        APP_INSPECTION_ACCOUNTING_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/corrections.js".into(),
        APP_INSPECTION_CORRECTIONS_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/disclosure.js".into(),
        APP_INSPECTION_DISCLOSURE_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/proof-drawer.js".into(),
        APP_INSPECTION_PROOF_DRAWER_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/shared.js".into(),
        APP_INSPECTION_SHARED_JS.as_bytes().to_vec(),
    );
    assets.insert(
        "app/inspection/timeline.js".into(),
        APP_INSPECTION_TIMELINE_JS.as_bytes().to_vec(),
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
        assert!(assets.contains(&"app/presets.js"));
        assert!(assets.contains(&"app/agreement.js"));
        assert!(assets.contains(&"app/negotiation.js"));
        assert!(assets.contains(&"app/occurrence.js"));
        assert!(assets.contains(&"app/hierarchy.js"));
        assert!(assets.contains(&"app/bulk.js"));
        assert!(assets.contains(&"app/delivery.js"));
        assert!(assets.contains(&"app/delivery/model.js"));
        assert!(assets.contains(&"app/delivery/recipients.js"));
        assert!(assets.contains(&"app/delivery/receipts.js"));
        assert!(assets.contains(&"app/delivery/conflicts.js"));
        assert!(assets.contains(&"app/inspection.js"));
        assert!(assets.contains(&"app/inspection/accounting.js"));
        assert!(assets.contains(&"app/inspection/corrections.js"));
        assert!(assets.contains(&"app/inspection/disclosure.js"));
        assert!(assets.contains(&"app/inspection/proof-drawer.js"));
        assert!(assets.contains(&"app/inspection/shared.js"));
        assert!(assets.contains(&"app/inspection/timeline.js"));
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
