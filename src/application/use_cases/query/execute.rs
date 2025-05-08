use std::io::Error;

use crate::application::providers::query::{
    execute::provider_query_execute, get_by_id::provider_query_get_by_id,
};

pub fn use_case_query_execute(id: u32) -> Result<(), Error> {
    futures::executor::block_on(async {
        let query = provider_query_get_by_id(id).await?;
        provider_query_execute(query.query).await
    })
}
