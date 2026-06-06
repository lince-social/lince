use crate::domain::lince_package::package_id_from_filename;

const TRANSFER_PACKAGE_ID: &str = "transfer";

pub(crate) fn is_supported_transfer_package_filename(package_name: &str) -> bool {
    package_id_from_filename(package_name) == TRANSFER_PACKAGE_ID
}

#[cfg(test)]
mod tests {
    use super::is_supported_transfer_package_filename;

    #[test]
    fn accepts_html_filename() {
        assert!(is_supported_transfer_package_filename("transfer.html"));
    }

    #[test]
    fn accepts_archive_filename() {
        assert!(is_supported_transfer_package_filename("transfer.lince"));
    }

    #[test]
    fn rejects_other_widgets() {
        assert!(!is_supported_transfer_package_filename("kanban.lince"));
    }
}
