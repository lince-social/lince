//! The serialized Protein read contract, independent of storage and execution.

mod decimal_operand;
mod expression;
mod include;
mod order;
mod predicate;
pub mod relation_context;
pub mod shader_inputs;

pub use expression::{Aggregate, AggregateOp, GroupBy, Protein, Source};
pub use include::{
    ExtensionInclude, FactsInclude, Include, LinkDirection, LinksInclude, MessageCursor,
    ProjectionInclude, PromisesInclude, ThreadsInclude,
};
pub use order::{LinkEndpoint, LinkOrder, Order};
pub use predicate::{DateComparison, Predicate, WorkDateField};
