use super::*;
use nucleus::social::gossip::{MAX_ENTRIES, Payload};

pub(super) async fn queue_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    payload: &Payload,
    kind: &str,
    identity: &str,
    peers: &[(String, String)],
    settings: &ServiceSettings,
) -> Result<(), EngineError> {
    let held: Option<(String, bool)> = store::sqlx::query_as(
        "SELECT hash,assigned FROM social_gossip_item WHERE kind=? AND document_hash=?",
    )
    .bind(kind)
    .bind(identity)
    .fetch_optional(&mut **tx)
    .await?;
    if held
        .as_ref()
        .is_some_and(|(_, assigned)| *assigned || peers.is_empty())
    {
        return Ok(());
    }
    let mut shuffled = peers.to_vec();
    for index in (1..shuffled.len()).rev() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random)
            .map_err(|_| invalid("Secure gossip selection is unavailable"))?;
        shuffled.swap(
            index,
            (u64::from_le_bytes(random) % (index as u64 + 1)) as usize,
        );
    }
    let previous:Vec<String>=store::sqlx::query_scalar("SELECT f.peer FROM social_gossip_forward f JOIN social_gossip_item i ON i.hash=f.hash WHERE i.post=? GROUP BY f.peer ORDER BY MAX(i.expires_at) DESC,f.peer LIMIT 3")
        .bind(&payload.proof().id).fetch_all(&mut **tx).await?;
    let mut selected: Vec<(String, String)> = previous
        .iter()
        .filter_map(|endpoint| shuffled.iter().find(|(_, id)| id == endpoint).cloned())
        .collect();
    for peer in shuffled {
        if selected.len() == 3 {
            break;
        }
        if !selected.iter().any(|(_, id)| id == &peer.1) {
            selected.push(peer);
        }
    }
    let entries: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_forward")
        .fetch_one(&mut **tx)
        .await?;
    let items: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_item")
        .fetch_one(&mut **tx)
        .await?;
    let bytes: i64 = store::sqlx::query_scalar(
        "SELECT COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_gossip_item",
    )
    .fetch_one(&mut **tx)
    .await?;
    let body = serde_json::to_string(payload)?;
    let capacity = if payload.control() {
        MAX_ENTRIES
    } else {
        MAX_ENTRIES * 9 / 10
    };
    let budget = settings.storage_bytes.min(32 * 1024 * 1024);
    let byte_capacity = if payload.control() {
        budget
    } else {
        budget * 9 / 10
    };
    if entries + selected.len() as i64 > capacity
        || items + i64::from(held.is_none()) > capacity
        || bytes as u64 + if held.is_none() { body.len() as u64 } else { 0 } > byte_capacity
    {
        return Err(invalid(
            "The retained gossip ledger is full; new forwarding waits without forgetting live deduplication or control reserves",
        ));
    }
    let hash = if let Some((hash, _)) = held {
        hash
    } else {
        let hash = gossip::payload_hash(payload)?;
        store::sqlx::query("INSERT INTO social_gossip_item(hash,post,kind,document_hash,body,expires_at,control) VALUES(?,?,?,?,?,?,?)")
            .bind(&hash).bind(&payload.proof().id).bind(kind).bind(identity).bind(body).bind(payload.expires_at()).bind(payload.control()).execute(&mut **tx).await?;
        hash
    };
    for (contact, endpoint) in &selected {
        store::sqlx::query("INSERT INTO social_gossip_forward(hash,peer,contact) VALUES(?,?,?)")
            .bind(&hash)
            .bind(endpoint)
            .bind(contact)
            .execute(&mut **tx)
            .await?;
    }
    if !selected.is_empty() {
        store::sqlx::query("UPDATE social_gossip_item SET assigned=1 WHERE hash=?")
            .bind(&hash)
            .execute(&mut **tx)
            .await?;
        if !payload.control() {
            store::sqlx::query("UPDATE social_gossip_scan SET error=NULL WHERE id=1")
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}
