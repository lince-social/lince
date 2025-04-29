use crate::{
    domain::entities::karma::Karma,
    infrastructure::database::repositories::karma::repository_karma_get_deliver,
};
use std::io::Error;

pub async fn provider_karma_get_deliver() -> Result<Vec<Karma>, Error> {
    repository_karma_get_deliver().await
}
