mod body;
mod script;
mod style;

use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    maud::{DOCTYPE, PreEscaped, html},
};

pub(crate) const FEATURE_FLAG: &str = "sand.lince_logo_led";

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "◈".into(),
        title: "Lince Logo LED".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Animated Lince logo with card-local lighting modes.".into(),
        details: "Click the logo to cycle its lighting mode. The selected mode is kept in board card state and does not create Records or Ledger Facts.".into(),
        initial_width: 4,
        initial_height: 4,
        requires_server: false,
        permissions: vec!["bridge_state".into()],
    };

    let document = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lince Logo LED" }
                script src="/board/frame.js" {}
                style { (PreEscaped(style::CSS)) }
            }
            body {
                (body::body())
                script { (PreEscaped(script::SCRIPT)) }
            }
        }
    }
    .into_string();

    LincePackage::new(Some("lince-logo-led.html".into()), manifest, document)
        .expect("lince logo LED official sand should render as a valid package")
}
