//! Record ordering by stored fields, assertion priorities, and links.

use std::{collections::HashMap, mem::swap};

use nucleus::{
    graph::topo_order_indices,
    performance::{Span, add},
};
use store::{
    Store,
    records::{RecordBody, RecordRow},
    sqlx::Error::Protocol,
};

use crate::{LinkEndpoint, LinkOrder, Order, ProteinError, assertion_order};

mod keys;
mod permutation;

use keys::Keys;
use permutation::{apply, prepare};

pub(super) async fn records<Body: RecordBody>(
    store: &Store,
    mut rows: Vec<RecordRow<Body>>,
    order: &[Order],
) -> Result<Vec<RecordRow<Body>>, ProteinError> {
    let _ordering = Span::new("protein-record-order-await");
    if fields(order).any(|field| matches!(field, "body" | "description"))
        && rows.iter().any(|row| row.body.loaded().is_none())
    {
        return Err(Protocol("Record body was not loaded for this sort".into()));
    }
    if rows.len() > 1 {
        let indices = prepare(
            rows.iter()
                .map(|row| (row.head.to_lowercase(), row.uid.as_str())),
            Ord::cmp,
        );
        apply(&mut rows, indices);
    }
    let keys = Keys::load(store, &rows, order).await?;
    let priority_fields: Vec<_> = fields(order)
        .filter(|field| field.starts_with('#'))
        .collect();
    let priorities = if priority_fields.is_empty() {
        assertion_order::Values::default()
    } else {
        let uids: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
        assertion_order::values(store, &uids, &priority_fields).await?
    };
    for key in order.iter().rev() {
        match key {
            Order::Asc(field) | Order::Desc(field) => keys.sort(
                &mut rows,
                field,
                matches!(key, Order::Desc(_)),
                priorities.get(field),
            ),
            Order::Link(rule) => sort_links(store, &mut rows, rule).await?,
        }
    }
    Ok(rows)
}

fn fields(order: &[Order]) -> impl Iterator<Item = &str> {
    order.iter().filter_map(|order| match order {
        Order::Asc(field) | Order::Desc(field) => Some(field.as_str()),
        Order::Link(_) => None,
    })
}

async fn sort_links<Body: RecordBody>(
    store: &Store,
    rows: &mut Vec<RecordRow<Body>>,
    rule: &LinkOrder,
) -> Result<(), ProteinError> {
    let family = store::vocabulary::meaning_family(&store.pool, &rule.kind, None).await?;
    let mut edges = store::assertions::edges_of_predicates(&store.pool, &family).await?;
    if rule.higher == LinkEndpoint::To {
        for edge in &mut edges {
            swap(&mut edge.from, &mut edge.to);
        }
    }
    let _ordering = Span::new("protein-record-link-order");
    add(
        "protein-record-link-candidates",
        u64::try_from(rows.len()).unwrap_or(u64::MAX),
    );
    let mut indices = {
        let uids: Vec<_> = rows.iter().map(|row| row.uid.as_str()).collect();
        let mut indices = topo_order_indices(&uids, &edges);
        let mut latest: HashMap<_, _> = uids
            .iter()
            .enumerate()
            .map(|(index, &uid)| (uid, index))
            .collect();
        if latest.len() != uids.len() {
            // Keep the last payload for each UID at its first topological emission.
            indices.retain_mut(|index| {
                let Some(source) = latest.remove(uids[*index]) else {
                    return false;
                };
                *index = source;
                true
            });
        }
        indices
    };
    let kept = indices.len();
    if kept < rows.len() {
        let mut selected = vec![false; rows.len()];
        for &index in &indices {
            selected[index] = true;
        }
        indices.extend((0..rows.len()).filter(|&index| !selected[index]));
        add(
            "protein-record-link-duplicates",
            u64::try_from(rows.len() - kept).unwrap_or(u64::MAX),
        );
    }
    apply(rows, indices);
    rows.truncate(kept);
    Ok(())
}
