//! Assemble Trust Analysis evidence from observed receipts and claim controls.

mod claims;
mod receipts;

pub use claims::{Claim, ClaimHead, HeadState, claim_heads};
pub use receipts::{Pending, Receipt, ReceiptGroup, receipts};
