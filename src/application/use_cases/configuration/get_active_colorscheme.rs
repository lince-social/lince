use crate::{
    application::providers::configuration::activate::provider_configuration_get_active,
    presentation::web::colorscheme::{
        catppuccin::machiatto::presentation_colorscheme_catppuccin_machiatto,
        general::default::presentation_colorscheme_general_default,
        mono::{
            black_in_white::presentation_colorscheme_mono_black_in_white,
            white_in_black::presentation_colorscheme_mono_white_in_black,
        },
    },
};

pub async fn use_case_configuration_get_active_colorscheme() -> &'static str {
    let style = match provider_configuration_get_active()
        .await
        .ok()
        .and_then(|c| c.style)
    {
        Some(s) => s,
        None => return presentation_colorscheme_general_default(),
    };

    match style.as_str() {
        "white_in_black" => presentation_colorscheme_mono_white_in_black(),
        "black_in_white" => presentation_colorscheme_mono_black_in_white(),
        "catppuccin_machiatto" => presentation_colorscheme_catppuccin_machiatto(),
        _ => presentation_colorscheme_general_default(),
    }
}
