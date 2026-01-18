use injection::cross_cutting::InjectedServices;

pub async fn get_active_colorscheme(services: InjectedServices) -> String {
    match services
        .repository
        .configuration
        .get_active()
        .await
        .ok()
        .map(|c| c.style)
    {
        Some(s) => s,
        None => "general_default".to_string(),
    }
}
