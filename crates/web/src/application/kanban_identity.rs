use crate::application::relations_identity::is_supported_relations_package_filename;
use crate::domain::lince_package::normalize_package_filename;

pub(crate) const KANBAN_PACKAGE_FILENAME: &str = "kanban-record-view.html";

const KANBAN_PACKAGE_FILENAME_ALIASES: [&str; 2] =
    [KANBAN_PACKAGE_FILENAME, "kanban_record_view.html"];

pub(crate) fn is_supported_kanban_package_filename(package_name: &str) -> bool {
    let normalized = normalize_package_filename(package_name);
    KANBAN_PACKAGE_FILENAME_ALIASES
        .iter()
        .any(|candidate| normalized == *candidate)
}

pub(crate) fn is_supported_graph_widget_filename(package_name: &str) -> bool {
    is_supported_kanban_package_filename(package_name)
        || is_supported_relations_package_filename(package_name)
}

#[cfg(test)]
mod tests {
    use super::is_supported_kanban_package_filename;

    #[test]
    fn accepts_current_official_filename() {
        assert!(is_supported_kanban_package_filename(
            "kanban-record-view.html"
        ));
    }

    #[test]
    fn accepts_legacy_filename_alias() {
        assert!(is_supported_kanban_package_filename(
            "kanban_record_view.html"
        ));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_kanban_package_filename("tasklist.html"));
    }

    #[test]
    fn graph_widget_accepts_relations_package() {
        assert!(super::is_supported_graph_widget_filename("relations.lince"));
    }
}
