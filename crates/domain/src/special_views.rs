pub const KARMA_ORCHESTRA_VIEW_QUERY: &str = "karma_orchestra";

pub fn normalize_special_view_query(query: &str) -> String {
    query.trim().to_lowercase().replace(['-', ' '], "_")
}

pub fn parse_creation_view_query(query: &str) -> Option<String> {
    let normalized = normalize_special_view_query(query);
    normalized
        .strip_prefix("create_view_")
        .or_else(|| normalized.strip_prefix("creation_view_"))
        .or_else(|| normalized.strip_prefix("create_modal_"))
        .or_else(|| normalized.strip_prefix("creation_modal_"))
        .or_else(|| normalized.strip_prefix("cv_"))
        .map(str::to_string)
}

pub fn is_karma_orchestra_view_query(query: &str) -> bool {
    normalize_special_view_query(query) == KARMA_ORCHESTRA_VIEW_QUERY
}

pub fn is_special_view_query(query: &str) -> bool {
    parse_creation_view_query(query).is_some()
        || matches!(
            normalize_special_view_query(query).as_str(),
            KARMA_ORCHESTRA_VIEW_QUERY | "karma_view" | "testing" | "command_buffer"
        )
}
