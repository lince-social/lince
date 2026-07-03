use crate::domain::lince_package::normalize_package_filename;

const HOME_MANAGER_PACKAGE_FILENAME: &str = "home-manager.html";

pub(crate) fn is_supported_home_manager_package_filename(package_name: &str) -> bool {
    normalize_package_filename(package_name) == HOME_MANAGER_PACKAGE_FILENAME
}

#[cfg(test)]
mod tests {
    use super::is_supported_home_manager_package_filename;

    #[test]
    fn accepts_official_filename() {
        assert!(is_supported_home_manager_package_filename(
            "home-manager.html"
        ));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_home_manager_package_filename("finance.html"));
    }
}
