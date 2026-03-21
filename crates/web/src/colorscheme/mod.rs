pub mod catppuccin;
pub mod general;
pub mod mono;
pub mod perfect_blue;

pub fn resolve_colorscheme(name: &str) -> &'static str {
    match name {
        "catppuccin_macchiato" => {
            catppuccin::macchiato::presentation_colorscheme_catppuccin_macchiato()
        }
        "mono_black_in_white" => {
            mono::black_in_white::presentation_colorscheme_mono_black_in_white()
        }
        "mono_white_in_black" => {
            mono::white_in_black::presentation_colorscheme_mono_white_in_black()
        }
        "perfect_blue" => perfect_blue::presentation_colorscheme_perfect_blue(),
        "general_default" => general::default::presentation_colorscheme_general_default(),
        _ => general::default::presentation_colorscheme_general_default(),
    }
}
