use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn use_case_karma_get(services: InjectedServices) -> Vec<String> {
    services.providers.karma.get_deliver.get_all().await
}
