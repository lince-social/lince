//! Supported Transfer sort keys and the final UID tie-breaker.

use super::{hierarchy::TransferOutput, transfer_query_error};
use crate::{Order, ProteinError};
use std::cmp::Ordering;

pub(super) struct SortKey {
    field: Field,
    descending: bool,
}

enum Field {
    Uid,
    Slug,
    Head,
    Revision,
    Status,
}

impl SortKey {
    pub(super) fn parse(order: &[Order]) -> Result<Vec<Self>, ProteinError> {
        order.iter().map(Self::parse_key).collect()
    }

    fn parse_key(key: &Order) -> Result<Self, ProteinError> {
        let (field, descending) = match key {
            Order::Asc(field) => (field.as_str(), false),
            Order::Desc(field) => (field.as_str(), true),
            Order::Link(_) => {
                return Err(transfer_query_error(
                    "protein_transfer_unsupported_order",
                    "link ordering is only defined for Record rows",
                ));
            }
        };
        let field = match field {
            "uid" => Field::Uid,
            "slug" => Field::Slug,
            "head" => Field::Head,
            "revision" => Field::Revision,
            "status" => Field::Status,
            _ => {
                return Err(transfer_query_error(
                    "protein_transfer_unsupported_order",
                    field,
                ));
            }
        };
        Ok(Self { field, descending })
    }

    fn compare(&self, left: &TransferOutput, right: &TransferOutput) -> Ordering {
        let order = match self.field {
            Field::Uid => left.uid.cmp(&right.uid),
            Field::Slug => left.slug.cmp(&right.slug),
            Field::Head => left.head.cmp(&right.head),
            Field::Revision => left.revision.cmp(&right.revision),
            Field::Status => left.status.cmp(&right.status),
        };
        if self.descending {
            order.reverse()
        } else {
            order
        }
    }

    pub(super) fn sort(rows: &mut [TransferOutput], keys: &[Self]) {
        rows.sort_by(|left, right| {
            keys.iter()
                .map(|key| key.compare(left, right))
                .find(|order| !order.is_eq())
                .unwrap_or_else(|| left.uid.cmp(&right.uid))
        });
    }
}
