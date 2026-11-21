//! Record property discovery, read planning, and metadata projection.

mod attachments;
mod plan;
mod work;

pub(crate) use attachments::attach;
pub(crate) use plan::{predicates_require_body, requires_body, selected_include};

pub(crate) use protein_authority::query::record_schema::requests;
pub use protein_authority::query::record_schema::{FIELDS, Field, fields, selected};
