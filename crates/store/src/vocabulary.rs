//! Vocabulary storage for source meanings, terms, definitions, and local preferences.

mod dictionary;
mod display;
mod features;
mod locations;
mod preferences;
mod publications;
mod references;
mod resolution;
mod terms;
mod translations;
mod versions;

use crate::StoreError;

pub use dictionary::{
    apply_dictionary, apply_dictionary_on, dictionary_on, log_shared_on, sources,
    validate_dictionary, validate_source_parent_on,
};
pub use display::display_names;
pub use features::{
    FEATURES, bind_feature_on, feature_binding, feature_binding_on, feature_bindings,
    feature_preview_on, feature_read, feature_resolve, feature_write, feature_write_on,
    is_feature_on, meaning_family, validate_feature_assertion_on,
};
pub use locations::{apply_location_binding, location_bindings};
pub use preferences::{
    choose_term_on, hide_term_on, reset_terminology_on, set_terminology_context_on,
    terminology_context_on, terminology_owner, terminology_state, terminology_state_on,
    use_wording_on,
};
pub use publications::{
    PACKAGE_LIMIT, adopt_on, collection_details, create_vocabulary_on, publication_on, publish_on,
    read_package, require_local_concept_on, require_local_organ_on, require_local_vocabulary_on,
    retained_by_publication_on, validate_package, withdraw_on,
};
pub use references::{conversion_reference_on, current_meaning_reference_on, meaning_reference_on};
pub use resolution::{resolve, resolve_file_meaning};
pub use terms::{
    add_term_on, edit_term_on, get_term, language_tag, retire_term_on, term_text, terms, terms_on,
};
pub use translations::{
    export_translations_on, import_translations_on, preview_translations_on, read_translations,
};
pub use versions::{definition_versions, definitions_on, record_history_on, revise_definition_on};

fn invalid(message: &str) -> StoreError {
    StoreError::Protocol(message.into())
}
