use crate::domain::lince_package::package_id_from_filename;

const TRAIL_PACKAGE_IDS: [&str; 2] = ["trail", "trail_relation"];

pub(crate) fn is_supported_trail_package_filename(package_name: &str) -> bool {
    let package_id = package_id_from_filename(package_name);
    TRAIL_PACKAGE_IDS.contains(&package_id.as_str())
}

#[cfg(test)]
mod tests {
    use super::is_supported_trail_package_filename;

    #[test]
    fn accepts_archive_filename() {
        assert!(is_supported_trail_package_filename("trail.lince"));
    }

    #[test]
    fn accepts_html_alias() {
        assert!(is_supported_trail_package_filename("trail.html"));
    }

    #[test]
    fn accepts_legacy_filename() {
        assert!(is_supported_trail_package_filename("trail_relation.lince"));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_trail_package_filename("relations.lince"));
    }
}
