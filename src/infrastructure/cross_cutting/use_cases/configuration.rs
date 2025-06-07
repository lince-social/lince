use crate::application::use_cases::configuration::get_active_colorscheme::UseCaseConfigurationGetActiveColorscheme;

pub struct ConfigurationUseCases {
    pub get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme,
}

pub fn injection_use_cases_configuration() -> ConfigurationUseCases {
    ConfigurationUseCases {
        get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme {},
    }
}
