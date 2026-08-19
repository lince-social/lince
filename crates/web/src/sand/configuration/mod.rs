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

    /// C2c's surface. A budget nobody can inspect is a budget nobody can
    /// trust, and the doc's own rule is that a cluster landing its mechanism
    /// without its surface has shipped something its owner cannot try.
    #[test]
    fn the_storage_page_answers_how_big_this_lince_is() {
        assert!(HTML.contains("/host/storage"), "no way to read the usage");
        assert!(HTML.contains("/host/storage/budget"), "no way to set the ceiling");
        assert!(HTML.contains("budget-areas"), "per-area usage is the whole point");
        assert!(
            HTML.contains("on disk in total"),
            "the honest total has to include what is never evicted"
        );
    }

    /// An area reading zero because its consumer does not exist yet must not
    /// look like an area reading zero because it is empty.
    #[test]
    fn an_area_with_no_consumer_says_which_nothing_it_means() {
        assert!(HTML.contains("not built yet"));
        assert!(HTML.contains("area.live"));
    }
}
