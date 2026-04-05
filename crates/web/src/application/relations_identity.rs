use crate::domain::lince_package::package_id_from_filename;

const RELATIONS_PACKAGE_ID: &str = "relations";

pub(crate) fn is_supported_relations_package_filename(package_name: &str) -> bool {
    package_id_from_filename(package_name) == RELATIONS_PACKAGE_ID
}

#[cfg(test)]
mod tests {
    use super::is_supported_relations_package_filename;

    #[test]
    fn accepts_archive_filename() {
        assert!(is_supported_relations_package_filename("relations.lince"));
    }

    #[test]
    fn accepts_html_alias() {
        assert!(is_supported_relations_package_filename("relations.html"));
    }

    #[test]
    fn rejects_other_packages() {
        assert!(!is_supported_relations_package_filename("kanban-record-view.html"));
    }
}
