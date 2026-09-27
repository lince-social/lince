pub use lince_interface::tokens::*;

pub(crate) mod tests {
    use lince_interface::tokens::checks::*;

    crate::laboratory_cases! {
        every_compiled_default_has_a_unique_name_and_round_trips,
        schemes_preserve_overrides_at_every_level_and_reset_reveals_inheritance,
        invalid_colors_numbers_and_saved_types_are_rejected,
    }
}
