use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::colorscheme::{
        catppuccin::machiatto::presentation_colorscheme_catppuccin_machiatto,
        general::default::presentation_colorscheme_general_default,
        mono::{
            black_in_white::presentation_colorscheme_mono_black_in_white,
            white_in_black::presentation_colorscheme_mono_white_in_black,
        },
    },
};

pub struct UseCaseConfigurationGetActiveColorscheme;

impl UseCaseConfigurationGetActiveColorscheme {
    pub async fn execute(&self, services: InjectedServices) -> &'static str {
        let style = match services
            .providers
            .configuration
            .get_active()
            .await
            .ok()
            .map(|c| c.style)
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
}
