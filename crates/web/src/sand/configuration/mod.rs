use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.configuration";
const HTML: &str = include_str!("configuration.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "⚙".into(),
        title: "Configuration".into(),
        author: "Lince Labs".into(),
        version: "0.1.0".into(),
        description: "Configures this Cell through typed, paged properties.".into(),
        details: "A side-panel property editor for the local Cell. Pages separate identity, discovery, storage, and contact settings. Boolean properties use the LynxUI toggle, quantities use numeric inputs, and strings use text inputs. Discovery writes the same lince.discovery extension used by the Organ Sand and live wire supervisor.".into(),
        initial_width: 5,
        initial_height: 5,
        requires_server: false,
        permissions: vec!["bridge_state".into(), "protein_subscribe".into(), "act".into()],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("configuration.html".into()), manifest(), HTML)
        .expect("configuration official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::HTML;

    #[test]
    fn configuration_has_paged_typed_properties() {
        assert!(HTML.contains("data-config-page=\"discovery\""));
        assert!(HTML.contains("class=\"lynx-toggle"));
        assert!(HTML.contains("type=\"number\""));
        assert!(HTML.contains("action: \"set-extension\""));
        assert!(HTML.contains("namespace: \"lince.discovery\""));
    }
}
