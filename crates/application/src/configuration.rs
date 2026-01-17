use crate::{
    infrastructure::cross_cutting::InjectedServices,
    log,
    presentation::html::colorscheme::{
        catppuccin::macchiato::presentation_colorscheme_catppuccin_macchiato,
        general::default::presentation_colorscheme_general_default,
        mono::{
            black_in_white::presentation_colorscheme_mono_black_in_white,
            white_in_black::presentation_colorscheme_mono_white_in_black,
        },
    },
};

pub async fn get_active_colorscheme(services: InjectedServices) -> &'static str {
    let style = match services
        .repository
        .configuration
        .get_active()
        .await
        .inspect_err(|e| log!(e, "Error in get_active_colorscheme: {}", e.to_string()))
        .ok()
        .map(|c| c.style)
    {
        Some(s) => s,
        None => return presentation_colorscheme_general_default(),
    };

    match style.as_str() {
        "white_in_black" => presentation_colorscheme_mono_white_in_black(),
        "black_in_white" => presentation_colorscheme_mono_black_in_white(),
        "catppuccin_macchiato" => presentation_colorscheme_catppuccin_macchiato(),
        _ => presentation_colorscheme_general_default(),
    }
}
