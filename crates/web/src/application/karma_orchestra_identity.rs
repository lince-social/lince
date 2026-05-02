use crate::domain::lince_package::package_id_from_filename;

const KARMA_ORCHESTRA_PACKAGE_ID: &str = "karma_orchestra";

pub(crate) fn is_supported_karma_orchestra_package_filename(package_name: &str) -> bool {
    package_id_from_filename(package_name) == KARMA_ORCHESTRA_PACKAGE_ID
}

#[cfg(test)]
mod tests {
    use super::is_supported_karma_orchestra_package_filename;

    #[test]
    fn accepts_archive_filename() {
        assert!(is_supported_karma_orchestra_package_filename(
            "karma_orchestra.lince"
        ));
    }

    #[test]
    fn accepts_html_alias() {
        assert!(is_supported_karma_orchestra_package_filename(
            "karma_orchestra.html"
        ));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_karma_orchestra_package_filename(
            "trail_relation.lince"
        ));
    }
}
