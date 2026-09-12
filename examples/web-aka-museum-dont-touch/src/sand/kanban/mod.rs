use crate::domain::lince_package::{LincePackage, PackageManifest};

pub(crate) const FEATURE_FLAG: &str = "sand.kanban_record_view";

const HTML: &str = include_str!("kanban.html");

pub(crate) fn manifest() -> PackageManifest {
    PackageManifest {
        icon: "▥".into(),
        title: "Kanban".into(),
        author: "Lince Labs".into(),
        version: "2.0.0".into(),
        description: "Live kanban board over Protein reads and typed Actions.".into(),
        details:
            "Subscribes to a records Protein through the widget bridge, buckets records into columns, moves cards with typed Actions, and emits the recordClicked ABI event so a Record sand can show details."
                .into(),
        initial_width: 6,
        initial_height: 6,
        requires_server: false,
        permissions: vec![
            "bridge_state".into(),
            "protein_subscribe".into(),
            "act".into(),
        ],
    }
}

pub(crate) fn package() -> LincePackage {
    LincePackage::new(Some("kanban.html".into()), manifest(), HTML)
        .expect("kanban official sand should render as a valid package")
}

#[cfg(test)]
mod tests {
    use super::HTML;

    #[test]
    fn a_locked_card_body_says_it_is_locked_instead_of_rendering_ciphertext() {
        assert!(HTML.contains("import(\"/board/vault.js\")"));
        assert!(HTML.contains("Vault.isLocked(row.body || \"\")"));
        assert!(HTML.contains("el.textContent = Vault.LOCKED_LABEL;"));
    }
}
