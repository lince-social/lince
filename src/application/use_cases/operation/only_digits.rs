use crate::infrastructure::cross_cutting::InjectedServices;
use std::io::Error;

pub struct UseCaseOnlyDigits {}

impl UseCaseOnlyDigits {
    pub async fn execute(&self, services: InjectedServices, id: u32) -> Result<(), Error> {
        services.providers.record.set_quantity(id, 0.0).await?;

        Ok(())
    }
}
