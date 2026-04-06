use crate::domain::lince_package::package_id_from_filename;

const TRAIL_RELATION_PACKAGE_ID: &str = "trail_relation";

pub(crate) fn is_supported_trail_package_filename(package_name: &str) -> bool {
    package_id_from_filename(package_name) == TRAIL_RELATION_PACKAGE_ID
}

#[cfg(test)]
mod tests {
    use super::is_supported_trail_package_filename;

    #[test]
    fn accepts_archive_filename() {
        assert!(is_supported_trail_package_filename("trail_relation.lince"));
    }

    #[test]
    fn accepts_html_alias() {
        assert!(is_supported_trail_package_filename("trail_relation.html"));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_trail_package_filename("relations.lince"));
    }
}
