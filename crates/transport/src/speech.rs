// non-lean-yet
// Scope: trait Service.
// BeautifulRust (baseline lines):
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   4, 5.

pub use fiote::speech::{Job, Request, Status};

#[async_trait::async_trait]
pub trait Service: Send + Sync {
    async fn handle(&self, request: Request) -> Result<Status, String>;
}
